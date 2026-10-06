/**
 * Conformance harness for the web engine.
 *
 * Runs the same assertions as the Rust test suite, so the two implementations
 * cannot drift apart silently. Run with: node reference/check-web.mjs
 */

import { CATALOG, checkFrame, compute, selfTest, buildIndex, buildFrame, frameCodeword, runBurstBatch } from '../web/crc.js';

let failures = 0;
const check = (name, condition, detail = '') => {
  if (!condition) {
    failures += 1;
    console.log(`  FAIL  ${name}${detail ? ` — ${detail}` : ''}`);
  } else {
    console.log(`  ok    ${name}`);
  }
};

console.log('catalogue check values');
for (const r of selfTest().results) {
  check(
    r.name,
    r.pass,
    `got ${r.got.toString(16)} expected ${r.expected.toString(16)}`,
  );
}

console.log('\nknown vectors');
const nine = new TextEncoder().encode('123456789');
const c32 = CATALOG.find((s) => s.name === 'CRC-32/ISO-HDLC');
check('CRC-32/ISO-HDLC("123456789") = 0xCBF43926', compute(c32, nine) === 0xcbf43926);
const ccitt = CATALOG.find((s) => s.name === 'CRC-16/CCITT-FALSE');
check('CRC-16/CCITT-FALSE("123456789") = 0x29B1', compute(ccitt, nine) === 0x29b1);

console.log('\nempty input is init XOR xorout');
for (const spec of CATALOG) {
  check(`${spec.name}`, compute(spec, new Uint8Array(0)) === ((spec.init ^ spec.xorout) & spec.mask));
}

console.log('\nself-built frames verify clean');
for (const spec of CATALOG) {
  const frame = buildFrame(spec, new TextEncoder().encode('hello'));
  const bits = frameCodeword(frame);
  const index = buildIndex(spec, bits.length, bits.slice(0, bits.length - spec.degree));
  const report = checkFrame(spec, bits, index);
  check(`${spec.name}`, report.verdict === 'valid', report.explanation);
}

console.log('\nsingle-bit flips are always detected');
for (const spec of CATALOG) {
  const frame = buildFrame(spec, new TextEncoder().encode('hello world'));
  const pristine = frameCodeword(frame);
  const split = pristine.length - spec.degree;
  const index = buildIndex(spec, pristine.length, pristine.slice(0, split));
  let detected = 0;
  let locatedWrong = 0;
  let repairsValid = 0;
  let repairAttempts = 0;
  for (let pos = 0; pos < pristine.length; pos += 1) {
    const bad = pristine.slice();
    bad[pos] ^= 1;
    const report = checkFrame(spec, bad, index);
    if (report.verdict === 'valid') continue;
    detected += 1;
    if (report.verdict === 'located') {
      if (report.location.len !== 1 || report.location.start !== pos) locatedWrong += 1;
      repairAttempts += 1;
      if (checkFrame(spec, report.repaired, index).verdict === 'valid'
        && report.repaired.every((b, i) => b === pristine[i])) {
        repairsValid += 1;
      }
    }
  }
  check(`${spec.name}: all ${pristine.length} flips detected`, detected === pristine.length, `${detected}`);
  check(`${spec.name}: no wrong location claimed`, locatedWrong === 0, `${locatedWrong}`);
  check(`${spec.name}: every repair restores the original`, repairsValid === repairAttempts, `${repairsValid}/${repairAttempts}`);
}

console.log('\nall bursts within deg(G) are detected');
for (const spec of CATALOG) {
  const frame = buildFrame(spec, new TextEncoder().encode('burst guard'));
  const pristine = frameCodeword(frame);
  const split = pristine.length - spec.degree;
  const index = buildIndex(spec, pristine.length, pristine.slice(0, split));
  let missed = 0;
  let total = 0;
  for (let len = 1; len <= Math.min(spec.degree, pristine.length); len += 1) {
    for (let start = 0; start <= pristine.length - len; start += 1) {
      for (let mask = 0; mask < 1 << len; mask += 1) {
        const bad = pristine.slice();
        for (let b = 0; b < len; b += 1) if ((mask >> b) & 1) bad[start + b] ^= 1;
        if (mask === 0) continue;
        total += 1;
        if (checkFrame(spec, bad, index).verdict === 'valid') missed += 1;
      }
    }
  }
  check(`${spec.name}: 0 missed of ${total} bursts`, missed === 0, `${missed} missed`);
}

console.log('\nMonte-Carlo within the guarantee');
for (const name of ['CRC-16/CCITT-FALSE', 'CRC-32/ISO-HDLC']) {
  const spec = CATALOG.find((s) => s.name === name);
  const payload = new TextEncoder().encode('monte carlo payload');
  let missed = 0;
  for (let len = 1; len <= spec.degree; len += 1) {
    const r = runBurstBatch(spec, payload, len, 500, 0xdeadbeef);
    missed += r.undetected;
  }
  check(`${name}: zero undetected`, missed === 0, `${missed}`);
}

console.log(`\n${failures === 0 ? 'ALL CHECKS PASSED' : `${failures} FAILURE(S)`}`);
process.exit(failures === 0 ? 0 : 1);
