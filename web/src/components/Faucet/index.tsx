import AddressBalance from "@components/AddressBalance";
import LoaderWithLabel from "@components/LoaderWithLabel";
import { useFaucetTransactions } from "@context/FaucetTransactionsContext";
import { faucetTokens, getBalance, getMonetaryConfig } from "@api/index";
import { formatDisplayAtomic, isEligible } from "@utils/amounts";
import ValidationMessagesList, {
    TValidationMessage,
} from "@components/ValidationMessagesList";
import {
    FaucetCoins,
    FaucetPhases,
} from "@utils/constants";
import {
    type ReactElement,
    useCallback,
    useEffect,
    useState,
    useRef,
} from "react";
import "./style.css";

export interface IFaucetProps {
    address: string;
}

const Faucet = ({ address }: IFaucetProps): ReactElement => {
    const [errorsList, setErrorsList] = useState<TValidationMessage[]>([]);
    const [faucetAmount, setFaucetAmount] = useState<string | null>(null);
    const [eligibilityCeiling, setEligibilityCeiling] = useState<string | null>(null);
    const [addressBalance, setAddressBalance] = useState<string | null>(null);
    const [isAddressBalanceFetching, setIsAddressBalanceFetching] =
        useState<boolean>(false);
    const [faucetPhase, setFaucetPhase] = useState<FaucetPhases>(
        FaucetPhases.ELIGIBILITY_TEST
    );

    const { isFlowCompleted, setIsFlowCompleted, setLastTransactionHash } =
        useFaucetTransactions();

    const requestIdRef = useRef(0);

    const updateAddressBalance = useCallback(async () => {
        const requestId = ++requestIdRef.current;

        if (!address) {
            setAddressBalance(null);
            setFaucetPhase(FaucetPhases.ELIGIBILITY_TEST);
            setIsFlowCompleted(false);
            return;
        }

        try {
            setIsAddressBalanceFetching(true);
            setIsFlowCompleted(false);

            const [balance, config] = await Promise.all([getBalance(address), getMonetaryConfig()]);

            if (requestId !== requestIdRef.current) {
                return;
            }

            if (balance == null) {
                throw new Error(
                    "Unable to acquire address balance. Please try again…"
                );
            }

            const formattedBalance = formatDisplayAtomic(balance, config.decimals);

            setAddressBalance(formattedBalance);
            setEligibilityCeiling(formatDisplayAtomic(config.ceiling.toString(), config.decimals));
            setFaucetAmount(formatDisplayAtomic(config.amount.toString(), config.decimals));

            if (isEligible(balance, config.ceiling)) {
                setFaucetPhase(FaucetPhases.READY_TO_CLAIM);
                setErrorsList([]);
            } else {
                setFaucetPhase(FaucetPhases.ELIGIBILITY_TEST);
                setErrorsList([`Your balance must be below ${formatDisplayAtomic(config.ceiling.toString(), config.decimals)} ASI to claim tokens.`]);
            }
        } catch (error) {
            if (requestId !== requestIdRef.current) {
                return;
            }

            console.error(error);

            const message =
                error instanceof Error
                    ? error.message
                    : String(error ?? "Something went wrong...");

            setErrorsList([message]);
            setFaucetPhase(FaucetPhases.ERROR);
        } finally {
            if (requestId === requestIdRef.current) {
                setIsAddressBalanceFetching(false);
            }
        }
    }, [address, setIsFlowCompleted]);

    useEffect(() => {
        setErrorsList([]);
        setFaucetPhase(FaucetPhases.ELIGIBILITY_TEST);
        updateAddressBalance();
    }, [address, updateAddressBalance, isFlowCompleted]);

    const faucet = async (): Promise<void> => {
        try {
            setFaucetPhase(FaucetPhases.IN_PROGRESS);

            const deployId: string = await faucetTokens(address);

            if (!deployId) {
                throw new Error(
                    "Unable to acquire 'deploy_id'. Please try again..."
                );
            }

            setLastTransactionHash(deployId);
            setFaucetPhase(FaucetPhases.COMPLETED);
        } catch (error) {
            console.error(error);

            const message =
                error instanceof Error
                    ? error.message
                    : String(error ?? "Something went wrong...");

            setErrorsList([message]);
            setFaucetPhase(FaucetPhases.ERROR);
        }
    };

    if (isAddressBalanceFetching) {
        return (
            <LoaderWithLabel
                label="Fetching address balance..."
                aria-busy="true"
            />
        );
    }

    if (faucetPhase === FaucetPhases.IN_PROGRESS) {
        return (
            <LoaderWithLabel label="Initiating faucet..." aria-busy="true" />
        );
    }

    return (
        <div className="faucet">
            <div className="faucet-content">
                {addressBalance != null && (
                    <div className="faucet-with-balance">
                        <div className="address-balance-block">
                            <AddressBalance
                                value={addressBalance}
                                coin={FaucetCoins.ASI}
                            />
                        </div>

                        <div className="faucet-initiator">
                            {eligibilityCeiling !== null && (
                                <p>Receive {faucetAmount} ASI when your balance is below {eligibilityCeiling} ASI.
                                    One claim at a time; retries resume a pending claim.</p>
                            )}
                            <button
                                onClick={faucet}
                                disabled={
                                    faucetPhase !== FaucetPhases.READY_TO_CLAIM
                                }
                                aria-disabled={
                                    faucetPhase !== FaucetPhases.READY_TO_CLAIM
                                }
                            >
                                FAUCET
                            </button>
                        </div>
                    </div>
                )}

                <div>
                    {faucetPhase === FaucetPhases.COMPLETED && (
                        <div className="faucet-success" role="status">
                            Tokens requested successfully.
                        </div>
                    )}

                    {faucetPhase === FaucetPhases.ERROR && (
                        <div className="faucet-failed" role="alert">
                            Request failed.
                        </div>
                    )}

                    <div className="faucet-errors">
                        <ValidationMessagesList
                            validationMessages={errorsList}
                        />
                    </div>
                </div>
            </div>
        </div>
    );
};

export default Faucet;
