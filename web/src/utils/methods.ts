import { REV_DECIMALS } from "./config";
import { decimalToAtomic, formatAtomic } from "./amounts";

const convertCogsToBalance = (cogs: unknown): string => formatAtomic(cogs, REV_DECIMALS);
const convertBalanceToCogs = (balance: string): bigint => decimalToAtomic(balance, REV_DECIMALS);

const isErrorStatus = (status: string) =>
    /_ERROR$/i.test(status) || /Error$/i.test(status);

export { convertBalanceToCogs, convertCogsToBalance, isErrorStatus };
