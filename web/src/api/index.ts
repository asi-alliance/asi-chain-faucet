import { BASE_URL } from "@utils/config";
import { parseAtomic, validateDecimals } from "@utils/amounts";

export async function getMonetaryConfig() {
    const response = await fetch(`${BASE_URL}/config`);
    if (!response.ok) throw new Error("Faucet amount configuration is unavailable");
    const config = await response.json();
    return {
        decimals: validateDecimals(config.native_token_decimals),
        amount: parseAtomic(config.faucet_amount),
        ceiling: parseAtomic(config.eligibility_ceiling),
    };
}

export interface IStatusResponse {
    status: string;
    msg?: string;
}

export const getBalance = async (walletAddress: string): Promise<string> => {
    const response = await fetch(`${BASE_URL}/balance/${walletAddress}`, {
        method: "GET",
        mode: "cors",
        credentials: "omit",
        headers: { Accept: "application/json" },
    });

    if (!response.ok) {
        throw new Error(`Error fetching balance: ${response.statusText}`);
    }

    const data = await response.json();

    return data?.balance;
};

export const faucetTokens = async (walletAddress: string): Promise<string> => {
    const response = await fetch(`${BASE_URL}/transfer`, {
        method: "POST",
        mode: "cors",
        credentials: "omit",
        headers: {
            "Content-Type": "application/json",
            Accept: "application/json",
        },
        body: JSON.stringify({ to_address: walletAddress }),
    });

    const data = await response.json();

    if (!response.ok) {
        throw new Error(
            `Error on faucet: ${data?.details || response.statusText}`
        );
    }

    return data?.deploy_id;
};

export const getTransactionStatus = async (
    deployId: string
): Promise<IStatusResponse> => {
    const response = await fetch(`${BASE_URL}/deploy/${deployId}`, {
        method: "GET",
        mode: "cors",
        credentials: "omit",
        headers: { Accept: "application/json" },
    });

    if (!response.ok) {
        throw new Error(`Error on check deploy_id: ${response.statusText}`);
    }

    const data = await response.json();

    return data;
};
