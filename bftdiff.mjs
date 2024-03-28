#!/usr/bin/env node
import { readFileSync } from 'node:fs';

function parseLine(line) {
  const match = line.match(
    /^transmission (?<tid>[^ ]+) = \((?<tx>.+), \{(?<certs>[^}]*)\}\)$/
  );
  if (!match) {
    console.error('invalid line', line);
    process.exit(1);
  }
  const { tid, tx, certs } = match.groups;
  return { tid, tx, certs: certs.split(', ') };
}

const a = readFileSync('damon.bft.txt', 'utf-8')
  .split('\n')
  .filter(Boolean)
  .map(parseLine);
const b = readFileSync('3111.bft.txt', 'utf-8')
  .split('\n')
  .filter(Boolean)
  .map(parseLine);

const aMap = Object.fromEntries(a.map(x => [x.tid, x]));
const bMap = Object.fromEntries(b.map(x => [x.tid, x]));

console.log(a.length, b.length);

let aMiss = 0,
  bMiss = 0,
  diffTx = 0,
  diffCerts = 0,
  aCertExtra = 0,
  bCertExtra = 0;
let shared = new Set();
for (const k in aMap) {
  if (!(k in bMap)) {
    bMiss++;
    continue;
  }
  shared.add(k);
  if (aMap[k].tx !== bMap[k].tx) {
    diffTx++;
  }
  if (aMap[k].certs.join(',') !== bMap[k].certs.join(',')) {
    const aCerts = new Set(aMap[k].certs);
    const bCerts = new Set(bMap[k].certs);

    let same = true;
    for (const c of aMap[k].certs) {
      if (!bCerts.has(c)) {
        aCertExtra++;
        same = false;
      }
    }
    for (const c of bMap[k].certs) {
      if (!aCerts.has(c)) {
        bCertExtra++;
        same = false;
      }
    }
    if (!same) {
      diffCerts++;
    }
  }
}
for (const k in bMap) {
  if (!(k in aMap)) {
    aMiss++;
    continue;
  }
  shared.add(k);
}

console.log('aMiss', aMiss, 'bMiss', bMiss, 'shared', shared.size);
console.log('diffTx', diffTx, 'diffCerts', diffCerts);
console.log('aCertExtra', aCertExtra, 'bCertExtra', bCertExtra);
