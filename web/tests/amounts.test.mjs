import assert from 'node:assert/strict';
import test from 'node:test';
import { decimalToAtomic, formatAtomic, parseAtomic, isEligible, UINT256_MAX } from '../src/utils/amounts.ts';

for (const decimals of [8, 18]) {
    test(`exact amounts and ceiling boundary at ${decimals} decimals`, () => {
        const unit = `0.${'0'.repeat(decimals - 1)}1`;
        assert.equal(formatAtomic('1', decimals), unit);
        assert.equal(decimalToAtomic(unit, decimals), 1n);
        assert.equal(decimalToAtomic(formatAtomic('9007199254740993', decimals), decimals), 9007199254740993n);
        const ceiling = decimalToAtomic('10', decimals);
        assert.equal(isEligible((ceiling - 1n).toString(), ceiling), true);
        assert.equal(isEligible(ceiling.toString(), ceiling), false);
        assert.equal(isEligible((ceiling + 1n).toString(), ceiling), false);
    });
}
test('wide amounts and invalid balance responses fail without rounding', () => {
    assert.equal(decimalToAtomic('10.000000000000000001', 18), 10000000000000000001n);
    assert.equal(parseAtomic(UINT256_MAX.toString()), UINT256_MAX);
    for (const value of [null, undefined, 9007199254740993, '-1', 'NaN', '1.0', '1e18', '01', (UINT256_MAX + 1n).toString()]) {
        assert.throws(() => parseAtomic(value));
    }
    for (const decimals of [-1, 19, 1.5, NaN]) assert.throws(() => formatAtomic('1', decimals));
    assert.throws(() => decimalToAtomic('0.000000001', 8));
});
