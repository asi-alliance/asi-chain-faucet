export const UINT256_MAX = (1n << 256n) - 1n;

export function validateDecimals(decimals: number): number {
    if (!Number.isInteger(decimals) || decimals < 0 || decimals > 18) {
        throw new Error("Token decimals must be an integer in 0..18");
    }
    return decimals;
}

export function parseAtomic(value: unknown): bigint {
    if (typeof value !== "string" || !/^(0|[1-9]\d*)$/.test(value)) {
        throw new Error("Balance must be an exact unsigned integer string");
    }
    const amount = BigInt(value);
    if (amount > UINT256_MAX) throw new Error("Amount exceeds uint256");
    return amount;
}

export function decimalToAtomic(value: string, decimals: number): bigint {
    validateDecimals(decimals);
    if (!/^\d+(\.\d+)?$/.test(value)) throw new Error("Invalid token amount");
    const [whole, fraction = ""] = value.split(".");
    if (fraction.length > decimals) throw new Error("Excess fractional precision");
    const amount = BigInt(whole) * 10n ** BigInt(decimals)
        + BigInt(fraction.slice(0, decimals).padEnd(decimals, "0") || "0");
    if (amount > UINT256_MAX) throw new Error("Amount exceeds uint256");
    return amount;
}

export function formatAtomic(value: unknown, decimals: number): string {
    validateDecimals(decimals);
    const raw = parseAtomic(value);
    const scale = 10n ** BigInt(decimals);
    const fraction = (raw % scale).toString().padStart(decimals, "0").replace(/0+$/, "");
    return `${raw / scale}${decimals && fraction ? `.${fraction}` : ""}`;
}

export function isEligible(value: unknown, ceiling: bigint): boolean {
    return parseAtomic(value) < ceiling;
}

// Display at most four fractional digits; eligibility always uses the exact amount.
export function formatDisplayAtomic(value: unknown, decimals: number): string {
    const [whole, fraction = ""] = formatAtomic(value, decimals).split(".");
    const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
    const visible = fraction.slice(0, 4).replace(/0+$/, "");
    const shortened = fraction.length > 4;
    if (shortened && whole === "0" && !visible) return "<0.0001";
    const display = visible ? `${grouped}.${visible}` : grouped;
    return shortened ? `≈${display}` : display;
}
