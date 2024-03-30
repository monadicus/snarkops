#!/usr/bin/env node

const pending = process.argv.filter(arg => arg.match(/^(\d+\.){3}\d+:\d+$/));

const seen = new Set();
const offline = new Set();
let peer;

while ((peer = pending.shift())) {
  if (seen.has(peer)) continue;
  seen.add(peer);

  console.log(`Scanning peers from ${peer}...`);
  try {
    const res = await fetch(`http://${peer}/mainnet/peers/all`);
    const peers = await res.json();

    for (let p of peers) {
      p = p.replace(/413/, '303');
      if (seen.has(p)) continue;

      pending.push(p);
      console.log(`- found ${p}`);
    }
  } catch (err) {
    console.error(`Error scanning ${peer}: ${err.message}`);
    offline.add(peer);
  }
}

for (const peer of seen) {
  if (offline.has(peer)) continue;

  try {
    const res = await fetch(`http://${peer}/mainnet/latest/block`);
    const block = await res.json();
    console.log(
      `${peer} - ${block.header.metadata.height} ${block.block_hash}`
    );
  } catch (err) {
    console.error(`Error fetching block from ${peer}: ${err.message}`);
  }
}
