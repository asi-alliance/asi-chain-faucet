//! Durable admission control. A signed deploy is journaled before any submission.
//! Locks are advisory OS locks, shared by processes using the same local volume.
use anyhow::{Context, Result};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
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
    Submission {
        deploy_id: String,
        error: anyhow::Error,
    },
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

// Bound both active admissions and detached persistence work after cancellation.
// A permit stays with the OS lock until its final close on the blocking pool.
const MAX_ACTIVE_CLAIMS: usize = 16;

fn persistence_slots() -> Arc<tokio::sync::Semaphore> {
    static SLOTS: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SLOTS
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(MAX_ACTIVE_CLAIMS)))
        .clone()
}

struct Journal {
    _lock: File,
    _permit: tokio::sync::OwnedSemaphorePermit,
    path: PathBuf,
    #[cfg(test)]
    save_gate: Option<Arc<IoGate>>,
    #[cfg(test)]
    fail_directory_sync: bool,
}

impl Journal {
    fn sync_directory(&self) -> Result<()> {
        let directory = File::open(self.path.parent().context("Missing claim directory")?)?;
        #[cfg(test)]
        anyhow::ensure!(
            !self.fail_directory_sync,
            "Injected claim directory sync failure"
        );
        directory.sync_all()?;
        Ok(())
    }
}

struct ClaimGuard(Option<Journal>, tokio::runtime::Handle);

impl Drop for ClaimGuard {
    fn drop(&mut self) {
        if let Some(journal) = self.0.take() {
            // Closing the locked file is filesystem I/O too. Do not release the
            // permit until the close has finished, including on cancellation.
            self.1.spawn_blocking(move || drop(journal));
        }
    }
}

impl ClaimGuard {
    async fn save(&mut self, reservation: Reservation) -> Result<Reservation> {
        let journal = self.0.take().expect("claim owns its journal");
        let (guard, reservation) = tokio::task::spawn_blocking(move || {
            let temporary = journal.path.with_extension("tmp");
            let mut file = File::create(&temporary)?;
            file.write_all(&serde_json::to_vec(&reservation)?)?;
            #[cfg(test)]
            if let Some(gate) = &journal.save_gate {
                gate.pause();
            }
            file.sync_all().context("Unable to persist claim")?;
            fs::rename(temporary, &journal.path)?;
            journal.sync_directory()?;
            // Cancellation cannot drop the recipient lock while this write is
            // still running. Submission is possible only after both fsyncs.
            Ok::<_, anyhow::Error>((
                Self(Some(journal), tokio::runtime::Handle::current()),
                reservation,
            ))
        })
        .await
        .context("Claim persistence worker failed")??;
        *self = guard;
        Ok(reservation)
    }

    async fn release(mut self) -> Result<()> {
        if let Some(journal) = self.0.take() {
            tokio::task::spawn_blocking(move || drop(journal))
                .await
                .context("Claim close worker failed")?;
        }
        Ok(())
    }
}

pub struct ClaimStore {
    directory: PathBuf,
    slots: Arc<tokio::sync::Semaphore>,
    #[cfg(test)]
    read_gate: Option<Arc<IoGate>>,
    #[cfg(test)]
    save_gate: Option<Arc<IoGate>>,
    #[cfg(test)]
    fail_directory_sync: bool,
}

#[cfg(test)]
struct IoGate {
    entered: tokio::sync::Notify,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

#[cfg(test)]
impl IoGate {
    fn new() -> (Arc<Self>, std::sync::mpsc::Sender<()>) {
        let (sender, receiver) = std::sync::mpsc::channel();
        (
            Arc::new(Self {
                entered: tokio::sync::Notify::new(),
                release: std::sync::Mutex::new(receiver),
            }),
            sender,
        )
    }

    fn pause(&self) {
        self.entered.notify_one();
        // A watchdog prevents a broken implementation from hanging the suite.
        self.release
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
    }
}

impl ClaimStore {
    pub async fn open(root: &Path, network: &str) -> Result<Self> {
        anyhow::ensure!(
            cfg!(unix),
            "Claim journals require a POSIX local filesystem; native Windows is unsupported"
        );
        let root = root.to_owned();
        let network = network.to_owned();
        let permit = persistence_slots().acquire_owned().await?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            Self::open_blocking(&root, &network)
        })
        .await
        .context("Claim directory worker failed")?
    }

    fn open_blocking(root: &Path, network: &str) -> Result<Self> {
        anyhow::ensure!(
            !network.is_empty() && network.len() <= 64,
            "FAUCET_NETWORK_ID must contain 1..64 bytes"
        );
        let namespace: String = network
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let directory = root.join(namespace);
        fs::create_dir_all(&directory)?;
        // Sync bottom-up through every ancestor: create_dir_all may have created
        // several levels, and syncing a directory does not persist its own entry
        // in its parent. A relative path's empty final ancestor means the cwd.
        // Include existing ancestors too, so concurrent openers cannot skip an
        // entry that another process has created but not yet made durable.
        for ancestor in directory.ancestors() {
            let ancestor = if ancestor.as_os_str().is_empty() {
                Path::new(".")
            } else {
                ancestor
            };
            File::open(ancestor)?.sync_all().with_context(|| {
                format!("Unable to persist claim directory {}", ancestor.display())
            })?;
        }
        Ok(Self {
            directory,
            slots: persistence_slots(),
            #[cfg(test)]
            read_gate: None,
            #[cfg(test)]
            save_gate: None,
            #[cfg(test)]
            fail_directory_sync: false,
        })
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
        let permit =
            self.slots.clone().try_acquire_owned().map_err(|_| {
                anyhow::anyhow!("Claim persistence capacity exhausted; retry later")
            })?;
        let path = self.directory.join(format!("{address}.json"));
        #[cfg(test)]
        let (read_gate, save_gate) = (self.read_gate.clone(), self.save_gate.clone());
        #[cfg(test)]
        let fail_directory_sync = self.fail_directory_sync;
        let (mut guard, existing) = tokio::task::spawn_blocking(move || {
            let lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(path.with_extension("lock"))
                .map_err(anyhow::Error::from)?;
            match lock.try_lock() {
                Ok(()) => {}
                Err(std::fs::TryLockError::WouldBlock) => return Err(ClaimError::Busy),
                Err(error) => return Err(anyhow::anyhow!(error).into()),
            }
            let journal = Journal {
                _lock: lock,
                _permit: permit,
                path,
                #[cfg(test)]
                save_gate,
                #[cfg(test)]
                fail_directory_sync,
            };
            #[cfg(test)]
            if let Some(gate) = read_gate {
                gate.pause();
            }
            let existing = match fs::read(&journal.path) {
                Ok(bytes) => Some(
                    serde_json::from_slice::<Reservation>(&bytes).map_err(anyhow::Error::from)?,
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(anyhow::Error::from(error).into()),
            };
            if existing.is_some() {
                // Repair durability if a previous writer renamed the journal
                // but failed its directory sync. Never resend merely visible bytes.
                File::open(&journal.path)
                    .and_then(|file| file.sync_all())
                    .map_err(anyhow::Error::from)?;
                journal.sync_directory()?;
            }
            Ok((
                ClaimGuard(Some(journal), tokio::runtime::Handle::current()),
                existing,
            ))
        })
        .await
        .context("Claim admission worker failed")??;
        let result = self
            .claim_locked(&mut guard, existing, node, address, amount, ceiling)
            .await;
        guard.release().await?;
        result
    }

    async fn claim_locked(
        &self,
        guard: &mut ClaimGuard,
        existing: Option<Reservation>,
        node: &impl ClaimNode,
        address: &str,
        amount: &BigUint,
        ceiling: &BigUint,
    ) -> Result<String, ClaimError> {
        // The guard stays held across eligibility, preparation and submission.
        if let Some(reservation) = existing {
            node_cli::vault::parse_amount(&reservation.amount).map_err(anyhow::Error::from)?;
            if node.deploy_id(&reservation.signed)? != reservation.deploy_id {
                return Err(anyhow::anyhow!("Corrupt claim journal").into());
            }
            match node.status(&reservation.deploy_id).await? {
                ClaimStatus::Pending => {
                    // Retry the journaled bytes and retain the reservation on failure.
                    node.submit(&reservation.signed).await.map_err(|error| {
                        ClaimError::Submission {
                            deploy_id: reservation.deploy_id.clone(),
                            error,
                        }
                    })?;
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
        let reservation = guard.save(reservation).await?;
        node.submit(&reservation.signed)
            .await
            .map_err(|error| ClaimError::Submission {
                deploy_id: deploy_id.clone(),
                error,
            })?;
        Ok(deploy_id)
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
        async fn store(&self, network: &str) -> ClaimStore {
            ClaimStore::open(&self.0, network).await.unwrap()
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
    async fn opens_relative_and_nested_journal_roots() {
        let directory = Directory(PathBuf::from(format!(
            ".faucet-claims-test-{}",
            uuid::Uuid::new_v4()
        )));
        let store = directory.store("network").await;
        assert!(store.directory.is_dir());
        let nested = ClaimStore::open(&directory.0.join("new/parents/claims"), "network")
            .await
            .unwrap();
        assert!(nested.directory.is_dir());
        assert_eq!(directory.store("network").await.directory, store.directory);
    }

    #[tokio::test]
    async fn concurrent_requests_and_pending_retries_never_sign_twice() {
        let directory = Directory::new();
        let store = directory.store("network").await;
        let second_process = directory.store("network").await;
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
        let wide = crate::utils::tokens_to_units("10.000000000000000001", 18).unwrap();
        assert!(matches!(
            directory
                .store("network")
                .await
                .claim(&node, "recipient", &wide, &wide)
                .await,
            Err(ClaimError::Submission { deploy_id, .. }) if deploy_id == "deploy1"
        ));
        let reopened = directory.store("network").await;
        assert!(matches!(
            reopened
                .claim(&node, "recipient", &amount(), &wide)
                .await,
            Err(ClaimError::Submission { deploy_id, .. }) if deploy_id == "deploy1"
        ));
        let journal_path = reopened.directory.join("recipient.json");
        let saved_bytes = fs::read(&journal_path).unwrap();
        let saved: Reservation = serde_json::from_slice(&saved_bytes).unwrap();
        assert_eq!(saved.amount, "10000000000000000001");
        assert_eq!(saved.deploy_id, "deploy1");
        assert_eq!(saved.signed, vec![1]);
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
        node.unavailable.store(true, Ordering::SeqCst);
        assert!(reopened
            .claim(&node, "recipient", &wide, &wide)
            .await
            .is_err());
        node.unavailable.store(false, Ordering::SeqCst);
        assert!(matches!(
            reopened
                .claim(&node, "recipient", &wide, &wide)
                .await,
            Err(ClaimError::Submission { deploy_id, .. }) if deploy_id == "deploy1"
        ));
        assert_eq!(fs::read(journal_path).unwrap(), saved_bytes);
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
        assert_eq!(*node.sent.lock().unwrap(), vec![vec![1]; 3]);
    }

    #[tokio::test]
    async fn finalization_requires_updated_balance_and_keeps_strict_boundary() {
        let directory = Directory::new();
        let store = directory.store("network").await;
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
            let store = directory.store("network").await;
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
    async fn directory_sync_failure_after_replacement_recovers_the_new_deploy() {
        let directory = Directory::new();
        let mut store = directory.store("network").await;
        store.slots = Arc::new(tokio::sync::Semaphore::new(1));
        let node = FakeNode::new();
        let value = amount();
        assert_eq!(
            store
                .claim(&node, "recipient", &value, &value)
                .await
                .unwrap(),
            "deploy1"
        );

        let path = store.directory.join("recipient.json");
        let previous = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        *node.state.lock().unwrap() = "Expired".into();

        // Inject the failure after admission; syncing the old journal must not
        // prevent this test from reaching the replacement write.
        let lock = OpenOptions::new()
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        lock.try_lock().unwrap();
        let mut guard = ClaimGuard(
            Some(Journal {
                _lock: lock,
                _permit: store.slots.clone().try_acquire_owned().unwrap(),
                path: path.clone(),
                save_gate: None,
                fail_directory_sync: true,
            }),
            tokio::runtime::Handle::current(),
        );
        let replacement = store
            .claim_locked(
                &mut guard,
                Some(previous),
                &node,
                "recipient",
                &value,
                &value,
            )
            .await;
        assert!(matches!(
            replacement,
            Err(ClaimError::Unavailable(ref error))
                if error.to_string() == "Injected claim directory sync failure"
        ));
        assert_eq!(*node.sent.lock().unwrap(), vec![vec![1]]);

        let saved_bytes = fs::read(&path).unwrap();
        let saved: Reservation = serde_json::from_slice(&saved_bytes).unwrap();
        assert_eq!(saved.deploy_id, "deploy2");
        assert_eq!(saved.signed, vec![2]);

        *node.state.lock().unwrap() = "Pending".into();
        assert_eq!(
            store
                .claim(&node, "recipient", &value, &value)
                .await
                .unwrap(),
            saved.deploy_id
        );
        assert_eq!(*node.sent.lock().unwrap(), vec![vec![1], saved.signed]);
        assert_eq!(node.prepared.load(Ordering::SeqCst), 2);
        assert_eq!(fs::read(path).unwrap(), saved_bytes);
    }

    #[tokio::test]
    async fn networks_and_recipients_are_independent_and_corruption_fails_closed() {
        let directory = Directory::new();
        let node = FakeNode::new();
        let store = directory.store("one").await;
        store
            .claim(&node, "recipient", &amount(), &amount())
            .await
            .unwrap();
        directory
            .store("two")
            .await
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
        let mut store = directory.store("network").await;
        store.slots = Arc::new(tokio::sync::Semaphore::new(1));
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
        let permit = tokio::time::timeout(std::time::Duration::from_secs(2), store.slots.acquire())
            .await
            .unwrap()
            .unwrap();
        drop(permit);
        assert_eq!(
            directory
                .store("network")
                .await
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
        let store = directory.store("network").await;
        let node = FakeNode::new();
        fs::create_dir(store.directory.join("recipient.tmp")).unwrap();
        assert!(store
            .claim(&node, "recipient", &amount(), &amount())
            .await
            .is_err());
        assert!(node.sent.lock().unwrap().is_empty());
    }
    #[tokio::test(flavor = "current_thread")]
    async fn delayed_journal_read_keeps_runtime_responsive_and_recipient_locked() {
        let directory = Directory::new();
        let mut store = directory.store("network").await;
        let other_process = directory.store("network").await;
        let (gate, release) = IoGate::new();
        store.read_gate = Some(gate.clone());
        let store = Arc::new(store);
        let node = Arc::new(FakeNode::new());
        let task = tokio::spawn({
            let store = store.clone();
            let node = node.clone();
            async move {
                store
                    .claim(node.as_ref(), "recipient", &amount(), &amount())
                    .await
            }
        });
        gate.entered.notified().await;
        tokio::time::timeout(std::time::Duration::from_millis(500), async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            assert!(matches!(
                other_process
                    .claim(node.as_ref(), "recipient", &amount(), &amount())
                    .await,
                Err(ClaimError::Busy)
            ));
            assert_eq!(
                other_process
                    .claim(node.as_ref(), "another", &amount(), &amount())
                    .await
                    .unwrap(),
                "deploy1"
            );
        })
        .await
        .unwrap();
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
        release.send(()).unwrap();
        assert_eq!(task.await.unwrap().unwrap(), "deploy2");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_during_fsync_keeps_lock_and_capacity_until_persisted() {
        let directory = Directory::new();
        let mut store = directory.store("network").await;
        let other_process = directory.store("network").await;
        // Isolate the capacity budget so other tests cannot influence saturation.
        store.slots = Arc::new(tokio::sync::Semaphore::new(1));
        let (gate, release) = IoGate::new();
        store.save_gate = Some(gate.clone());
        let store = Arc::new(store);
        let node = Arc::new(FakeNode::new());
        let task = tokio::spawn({
            let store = store.clone();
            let node = node.clone();
            async move {
                store
                    .claim(node.as_ref(), "recipient", &amount(), &amount())
                    .await
            }
        });
        gate.entered.notified().await;
        assert!(node.sent.lock().unwrap().is_empty());
        assert!(!store.directory.join("recipient.json").exists());
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(matches!(
            store
                .claim(node.as_ref(), "another", &amount(), &amount())
                .await,
            Err(ClaimError::Unavailable(_))
        ));
        assert!(matches!(
            other_process
                .claim(node.as_ref(), "recipient", &amount(), &amount())
                .await,
            Err(ClaimError::Busy)
        ));
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
        release.send(()).unwrap();
        // Capacity is released only after the detached writer has closed its lock.
        let permit = tokio::time::timeout(std::time::Duration::from_secs(2), store.slots.acquire())
            .await
            .unwrap()
            .unwrap();
        drop(permit);
        assert!(node.sent.lock().unwrap().is_empty());
        assert_eq!(
            other_process
                .claim(node.as_ref(), "recipient", &amount(), &amount())
                .await
                .unwrap(),
            "deploy1"
        );
        assert_eq!(*node.sent.lock().unwrap(), vec![vec![1]]);
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn submit_waits_for_journal_sync_completion() {
        let directory = Directory::new();
        let mut store = directory.store("network").await;
        let (gate, release) = IoGate::new();
        store.save_gate = Some(gate.clone());
        let node = FakeNode::new();
        let value = amount();
        let claim = store.claim(&node, "recipient", &value, &value);
        let check = async {
            gate.entered.notified().await;
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            assert!(node.sent.lock().unwrap().is_empty());
            release.send(()).unwrap();
        };
        let (result, ()) = tokio::join!(claim, check);
        assert_eq!(result.unwrap(), "deploy1");
        let saved: Reservation =
            serde_json::from_slice(&fs::read(store.directory.join("recipient.json")).unwrap())
                .unwrap();
        assert_eq!(*node.sent.lock().unwrap(), vec![saved.signed]);
    }
    #[tokio::test]
    async fn directory_sync_failure_after_rename_retries_only_the_saved_deploy() {
        let directory = Directory::new();
        let mut store = directory.store("network").await;
        store.fail_directory_sync = true;
        let value = amount();
        let node = FakeNode::new();

        let first = store.claim(&node, "recipient", &value, &value).await;
        assert!(matches!(
            first,
            Err(ClaimError::Unavailable(ref error))
                if error.to_string().contains("Injected claim directory sync failure")
        ));
        assert!(node.sent.lock().unwrap().is_empty());
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
        // The rename succeeded without a successful directory sync.
        let path = store.directory.join("recipient.json");
        assert!(!store.directory.join("recipient.tmp").exists());
        let saved_bytes = fs::read(&path).unwrap();
        let saved: Reservation = serde_json::from_slice(&saved_bytes).unwrap();
        assert_eq!(saved.deploy_id, "deploy1");
        assert_eq!(saved.signed, vec![1]);

        // Retry through the same store to exercise request recovery without
        // the directory sync that reopening the store would perform.
        let retry = store.claim(&node, "recipient", &value, &value).await;
        assert!(matches!(
            retry,
            Err(ClaimError::Unavailable(ref error))
                if error.to_string().contains("Injected claim directory sync failure")
        ));
        assert!(node.sent.lock().unwrap().is_empty());
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
        assert_eq!(fs::read(&path).unwrap(), saved_bytes);

        store.fail_directory_sync = false;
        assert_eq!(
            store
                .claim(&node, "recipient", &value, &value)
                .await
                .unwrap(),
            saved.deploy_id
        );
        assert_eq!(*node.sent.lock().unwrap(), vec![saved.signed]);
        assert_eq!(node.prepared.load(Ordering::SeqCst), 1);
        assert_eq!(fs::read(&path).unwrap(), saved_bytes);
    }

    #[tokio::test]
    async fn replacement_write_failure_preserves_previous_journal() {
        let directory = Directory::new();
        let store = directory.store("network").await;
        let node = FakeNode::new();
        store
            .claim(&node, "recipient", &amount(), &amount())
            .await
            .unwrap();
        let path = store.directory.join("recipient.json");
        let original = fs::read(&path).unwrap();
        *node.state.lock().unwrap() = "Expired".into();
        fs::create_dir(store.directory.join("recipient.tmp")).unwrap();
        assert!(store
            .claim(&node, "recipient", &amount(), &amount())
            .await
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(*node.sent.lock().unwrap(), vec![vec![1]]);
    }
}
