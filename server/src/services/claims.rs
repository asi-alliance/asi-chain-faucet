//! Durable admission control. A signed deploy is journaled before any submission.
//! Locks are advisory OS locks, shared by processes using the same local volume.
use anyhow::{Context, Result};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) trait ClaimNode {
    /// With a block, require current finalized state at or beyond its inclusion.
    async fn balance(&self, address: &str, block: Option<&str>) -> Result<String>;
    async fn prepare(&self, address: &str) -> Result<Vec<u8>>;
    fn deploy_id(&self, signed: &[u8]) -> Result<String>;
    async fn submit(&self, signed: &[u8]) -> Result<()>;
    async fn status(&self, id: &str) -> Result<ClaimStatus>;
}

pub(crate) enum ClaimStatus {
    Pending,
    Finalized(String),
    Failed,
    Expired,
}

#[derive(Debug)]
pub enum ClaimError {
    Busy,
    Ineligible,
    Unavailable(anyhow::Error),
}
impl From<anyhow::Error> for ClaimError {
    fn from(error: anyhow::Error) -> Self {
        Self::Unavailable(error)
    }
}

#[derive(Serialize, Deserialize)]
struct Reservation {
    amount: String,
    deploy_id: String,
    signed: Vec<u8>,
}

pub struct ClaimStore {
    directory: PathBuf,
}

impl ClaimStore {
    pub fn open(root: &Path, network: &str) -> Result<Self> {
        anyhow::ensure!(
            !network.is_empty() && network.len() <= 64,
            "FAUCET_NETWORK_ID must contain 1..64 bytes"
        );
        fs::create_dir_all(root)?;
        let namespace: String = network
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let directory = root.join(namespace);
        fs::create_dir_all(&directory)?;
        File::open(root)?.sync_all()?;
        if let Some(parent) = root.parent().filter(|p| !p.as_os_str().is_empty()) {
            File::open(parent)?.sync_all()?;
        }
        Ok(Self { directory })
    }

    pub(crate) async fn claim(
        &self,
        node: &impl ClaimNode,
        address: &str,
        amount: &BigUint,
        ceiling: &BigUint,
    ) -> Result<String, ClaimError> {
        if address.is_empty() || !address.bytes().all(|c| c.is_ascii_alphanumeric()) {
            return Err(anyhow::anyhow!("Invalid reservation address").into());
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join(format!("{address}.lock")))
            .map_err(anyhow::Error::from)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Err(ClaimError::Busy),
            Err(error) => return Err(anyhow::anyhow!(error).into()),
        }
        // The lock stays held across eligibility, preparation, journal and submission.
        let path = self.directory.join(format!("{address}.json"));
        let existing = match fs::read(&path) {
            Ok(bytes) => {
                Some(serde_json::from_slice::<Reservation>(&bytes).map_err(anyhow::Error::from)?)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(anyhow::Error::from(error).into()),
        };
        if let Some(reservation) = existing {
            node_cli::vault::parse_amount(&reservation.amount).map_err(anyhow::Error::from)?;
            if node.deploy_id(&reservation.signed)? != reservation.deploy_id {
                return Err(anyhow::anyhow!("Corrupt claim journal").into());
            }
            match node.status(&reservation.deploy_id).await? {
                ClaimStatus::Pending => {
                    // An unknown submission or crash can only resend the exact signed bytes.
                    // A second failure keeps the reservation intact, and the ID is still usable.
                    if let Err(error) = node.submit(&reservation.signed).await {
                        tracing::warn!(deploy_id = %reservation.deploy_id, %error, "Claim submission remains uncertain");
                    }
                    return Ok(reservation.deploy_id);
                }
                ClaimStatus::Finalized(block) => {
                    // Read finalized state at or beyond the canonical inclusion. An observer
                    // lagging finalization must never reopen the old eligibility window.
                    let balance = node.balance(address, Some(&block)).await?;
                    if !crate::config::is_eligible(&balance, ceiling)? {
                        return Err(ClaimError::Ineligible);
                    }
                }
                ClaimStatus::Failed | ClaimStatus::Expired => {}
            }
        }
        let balance = node.balance(address, None).await?;
        if !crate::config::is_eligible(&balance, ceiling)? {
            return Err(ClaimError::Ineligible);
        }
        let signed = node.prepare(address).await?;
        let deploy_id = node.deploy_id(&signed)?;
        let reservation = Reservation {
            amount: amount.to_string(),
            deploy_id: deploy_id.clone(),
            signed,
        };
        self.save(&path, &reservation)?;
        if let Err(error) = node.submit(&reservation.signed).await {
            tracing::warn!(%deploy_id, %error, "Claim submission uncertain; reservation retained");
        }
        Ok(deploy_id)
    }

    fn save(&self, path: &Path, reservation: &Reservation) -> Result<()> {
        let temporary = path.with_extension("tmp");
        let mut file = File::create(&temporary)?;
        file.write_all(&serde_json::to_vec(reservation)?)?;
        file.sync_all().context("Unable to persist claim")?;
        fs::rename(temporary, path)?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    };
    use tokio::sync::Notify;

    struct FakeNode {
        balance: Mutex<String>,
        finalized_balance: Mutex<String>,
        state: Mutex<String>,
        prepared: AtomicUsize,
        sent: Mutex<Vec<Vec<u8>>>,
        unknown: AtomicBool,
        unavailable: AtomicBool,
        delay: AtomicBool,
        entered: Notify,
        proceed: Notify,
    }
    impl FakeNode {
        fn new() -> Self {
            Self {
                balance: Mutex::new("0".into()),
                finalized_balance: Mutex::new("10".into()),
                state: Mutex::new("Pending".into()),
                prepared: AtomicUsize::new(0),
                sent: Mutex::new(vec![]),
                unknown: AtomicBool::new(false),
                unavailable: AtomicBool::new(false),
                delay: AtomicBool::new(false),
                entered: Notify::new(),
                proceed: Notify::new(),
            }
        }
    }
    impl ClaimNode for FakeNode {
        async fn balance(&self, _: &str, block: Option<&str>) -> Result<String> {
            anyhow::ensure!(
                !self.unavailable.load(Ordering::SeqCst),
                "observer unavailable"
            );
            Ok(if block.is_some() {
                self.finalized_balance.lock().unwrap()
            } else {
                self.balance.lock().unwrap()
            }
            .clone())
        }
        async fn prepare(&self, _: &str) -> Result<Vec<u8>> {
            let count = self.prepared.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(vec![count as u8])
        }
        fn deploy_id(&self, signed: &[u8]) -> Result<String> {
            Ok(format!("deploy{}", signed[0]))
        }
        async fn submit(&self, signed: &[u8]) -> Result<()> {
            self.sent.lock().unwrap().push(signed.to_vec());
            if self.delay.swap(false, Ordering::SeqCst) {
                self.entered.notify_one();
                self.proceed.notified().await;
            }
            anyhow::ensure!(!self.unknown.load(Ordering::SeqCst), "submission timed out");
            Ok(())
        }
        async fn status(&self, _: &str) -> Result<ClaimStatus> {
            anyhow::ensure!(
                !self.unavailable.load(Ordering::SeqCst),
                "observer unavailable"
            );
            Ok(match self.state.lock().unwrap().as_str() {
                "Finalized" => ClaimStatus::Finalized("canonical-block".into()),
                "Failed" => ClaimStatus::Failed,
                "Expired" => ClaimStatus::Expired,
                _ => ClaimStatus::Pending,
            })
        }
    }
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("faucet-claims-{}", uuid::Uuid::new_v4())))
        }
        fn store(&self, network: &str) -> ClaimStore {
            ClaimStore::open(&self.0, network).unwrap()
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn amount() -> BigUint {
        BigUint::from(10u32)
    }

    #[tokio::test]
    async fn concurrent_requests_and_pending_retries_never_sign_twice() {
        let directory = Directory::new();
        let store = directory.store("network");
        let second_process = directory.store("network");
        let node = FakeNode::new();
        node.delay.store(true, Ordering::SeqCst);
        let value = amount();
        let first = store.claim(&node, "recipient", &value, &value);
        let concurrent = async {
            node.entered.notified().await;
            assert!(matches!(
                second_process
                    .claim(&node, "recipient", &value, &value)
                    .await,
                Err(ClaimError::Busy)
            ));
            node.proceed.notify_one();
        };
        let (result, ()) = tokio::join!(first, concurrent);
        assert_eq!(result.unwrap(), "deploy1");
        for _ in 0..3 {
            assert_eq!(
                store
                    .claim(&node, "recipient", &value, &value)
                    .await
                    .unwrap(),
                "deploy1"
            );
        }
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
        assert!(node.sent.lock().unwrap().iter().all(|bytes| bytes == &[1]));
    }

    #[tokio::test]
    async fn restart_and_unknown_submission_retain_exact_wide_reservation() {
        let directory = Directory::new();
        let node = FakeNode::new();
        node.unknown.store(true, Ordering::SeqCst);
        let wide = node_cli::vault::tokens_to_dust("10.000000000000000001", 18).unwrap();
        assert_eq!(
            directory
                .store("network")
                .claim(&node, "recipient", &wide, &wide)
                .await
                .unwrap(),
            "deploy1"
        );
        let reopened = directory.store("network");
        assert_eq!(
            reopened
                .claim(&node, "recipient", &amount(), &wide)
                .await
                .unwrap(),
            "deploy1"
        );
        let saved: Reservation =
            serde_json::from_slice(&fs::read(reopened.directory.join("recipient.json")).unwrap())
                .unwrap();
        assert_eq!(saved.amount, "10000000000000000001");
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
        node.unavailable.store(true, Ordering::SeqCst);
        assert!(reopened
            .claim(&node, "recipient", &wide, &wide)
            .await
            .is_err());
        node.unavailable.store(false, Ordering::SeqCst);
        assert_eq!(
            reopened
                .claim(&node, "recipient", &wide, &wide)
                .await
                .unwrap(),
            "deploy1"
        );
    }

    #[tokio::test]
    async fn finalization_requires_updated_balance_and_keeps_strict_boundary() {
        let directory = Directory::new();
        let store = directory.store("network");
        let node = FakeNode::new();
        store
            .claim(&node, "recipient", &amount(), &amount())
            .await
            .unwrap();
        *node.state.lock().unwrap() = "Finalized".into();
        for balance in ["10", "11"] {
            *node.finalized_balance.lock().unwrap() = balance.into();
            assert!(matches!(
                store.claim(&node, "recipient", &amount(), &amount()).await,
                Err(ClaimError::Ineligible)
            ));
        }
        node.unavailable.store(true, Ordering::SeqCst);
        assert!(store
            .claim(&node, "recipient", &amount(), &amount())
            .await
            .is_err());
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
        node.unavailable.store(false, Ordering::SeqCst);
        *node.finalized_balance.lock().unwrap() = "9".into();
        assert_eq!(
            store
                .claim(&node, "recipient", &amount(), &amount())
                .await
                .unwrap(),
            "deploy2"
        );
    }

    #[tokio::test]
    async fn only_terminal_failure_or_expiration_allows_fresh_admission() {
        for terminal in ["Failed", "Expired"] {
            let directory = Directory::new();
            let store = directory.store("network");
            let node = FakeNode::new();
            store
                .claim(&node, "recipient", &amount(), &amount())
                .await
                .unwrap();
            *node.state.lock().unwrap() = terminal.into();
            *node.balance.lock().unwrap() = "10".into();
            assert!(matches!(
                store.claim(&node, "recipient", &amount(), &amount()).await,
                Err(ClaimError::Ineligible)
            ));
            *node.balance.lock().unwrap() = "9".into();
            assert_eq!(
                store
                    .claim(&node, "recipient", &amount(), &amount())
                    .await
                    .unwrap(),
                "deploy2"
            );
        }
    }

    #[tokio::test]
    async fn networks_and_recipients_are_independent_and_corruption_fails_closed() {
        let directory = Directory::new();
        let node = FakeNode::new();
        let store = directory.store("one");
        store
            .claim(&node, "recipient", &amount(), &amount())
            .await
            .unwrap();
        directory
            .store("two")
            .claim(&node, "recipient", &amount(), &amount())
            .await
            .unwrap();
        store
            .claim(&node, "another", &amount(), &amount())
            .await
            .unwrap();
        fs::write(store.directory.join("recipient.json"), "broken journal").unwrap();
        assert!(store
            .claim(&node, "recipient", &amount(), &amount())
            .await
            .is_err());
        assert_eq!(node.prepared.load(Ordering::SeqCst), 3);
    }
    #[tokio::test]
    async fn cancellation_during_submission_recovers_the_same_deploy() {
        let directory = Directory::new();
        let store = directory.store("network");
        let node = FakeNode::new();
        node.delay.store(true, Ordering::SeqCst);
        let amount = amount();
        {
            let claim = store.claim(&node, "recipient", &amount, &amount);
            tokio::pin!(claim);
            tokio::select! {
                result = &mut claim => panic!("submission should wait: {result:?}"),
                _ = node.entered.notified() => {},
            }
            // Dropping the request future simulates cancellation after journal commit.
        }
        assert_eq!(
            directory
                .store("network")
                .claim(&node, "recipient", &amount, &amount)
                .await
                .unwrap(),
            "deploy1"
        );
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn journal_write_failure_never_submits() {
        let directory = Directory::new();
        let store = directory.store("network");
        let node = FakeNode::new();
        fs::create_dir(store.directory.join("recipient.tmp")).unwrap();
        assert!(store
            .claim(&node, "recipient", &amount(), &amount())
            .await
            .is_err());
        assert!(node.sent.lock().unwrap().is_empty());
    }
}
