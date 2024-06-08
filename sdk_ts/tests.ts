import * as sdk from './index';
const { Command } = require('commander');

function delay(ms: number) {
	return new Promise(resolve => setTimeout(resolve, ms));
}

const program = new Command();
const snops = new sdk.Snops('http://localhost:1234');

program
	.version('0.0.1')
	.description('Tests for snarkOS via the snops SDK');

program
	.command('bft')
	.description('BFT Recovery Test')
	.argument('<time>', 'The time to wait before recovering BFT (e.g., 10s, 1m, 1h)')
	.action(async (time: string) => {
		const match = time.match(/^(\d+)([smh])$/);
		if (!match) {
			console.error('Invalid time format. Use 10s for seconds, 1m for minutes, or 1h for hours.');
			process.exit(1);
		}

		const value = parseInt(match[1]);
		const unit = match[2];

		let delayTime;
		switch (unit) {
			case 's':
				delayTime = value * 1000;
				break;
			case 'm':
				delayTime = value * 60 * 1000;
				break;
			case 'h':
				delayTime = value * 60 * 60 * 1000;
				break;
			default:
				throw new Error('Invalid unit');
		}

		console.log('Turning off validators...');
		const offline = await snops.env().action.offline("validator/*");
		console.log('Turned off the following validators:', offline);
		console.log(`Waiting ${value}${unit}...`);
		await delay(delayTime);
		const online = await snops.env().action.online("validator/*");
		console.log('Turned on the following validators:', online);
	});

program.parse();
