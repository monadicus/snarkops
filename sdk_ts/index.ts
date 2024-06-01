class Snops {
	private api: SnopsApi;

	constructor(url: string) {
		this.api = new SnopsApi(url);
	}

	agents(): Agents {
		return new Agents(this.api);
	}

	// env(env_id?: string): Env {
	// 	return new Env(this.api, env_id);
	// }
}

class SnopsApi {
	private static API: string = '/api/v1/';
	private url: string;

	constructor(url: string) {
		this.url = url;
	}

	async fetch<B, T>(method: string, url: string, body?: B): Promise<T> {
		const res = await fetch(`${this.url}${SnopsApi.API}${url}`, {
			method,
			body: body ? JSON.stringify({ body }) : null,
			headers: {
				'Content-Type': 'application/json'
			},
		});
		const blob = await res.json();
		return blob as T;
	}

	async get<T>(url: string): Promise<T> {
		return this.fetch<void, T>('GET', url)
	}

	async delete<T>(url: string): Promise<T> {
		return this.fetch<void, T>('DELETE', url)
	}

	async post<B, T>(url: string, body?: B): Promise<T> {
		return this.fetch<B, T>('POST', url, body)
	}

	async listAgents(): Promise<AgentStatus[]> {
		return await this.get('agents')
	}

	async getAgent(id: string): Promise<AgentStatus> {
		return this.get(`agents/${id}`);
	}

	async getAgentTps(id: string): Promise<string> {
		return this.get(`agents/${id}/tps`);
	}

	async findAgents(query: string, find: FindAgentsBody): Promise<AgentStatus[]> {
		return this.post(`agents/find`, find);
	}
}

interface AgentStatus {
	agent_id: string,
	is_connected: boolean,
	external_ip?: string,
	internal_ip?: string,
	state: any,
}

interface FindAgentsBody {
	mode: {
		client: boolean,
		compute: boolean,
		prover: boolean,
		validator: boolean,
	},
	env?: string,
	labels: string[],
	all: boolean,
	include_offline: boolean,
	local_pk: boolean,
}

class Agent {
	private status: AgentStatus;
	constructor(status: AgentStatus) {
		this.status = status;
	}
}

class Agents {
	private api: SnopsApi;

	constructor(api: SnopsApi) {
		this.api = api;
	}

	async list() {
		return await this.api.listAgents();
	}

	async get(agent_id: string) {
		return await this.api.getAgent(agent_id);
	}

	async tps(agent_id: string): Promise<string> {
		return await this.api.getAgentTps(agent_id);
	}

	find(): FindAgentBuilder {
		return new FindAgentBuilder(this.api);
	}
}

class FindAgentBuilder {
	private api: SnopsApi;
	private _client: boolean = false;
	private _compute: boolean = false;
	private _prover: boolean = false;
	private _validator: boolean = false;
	private _env?: string;
	private _all_env: boolean = false;
	private _labels: string[] = [];
	private _include_offline: boolean = false;
	private _local_pk: boolean = false;

	constructor(api: SnopsApi) {
		this.api = api;
	}

	client(): FindAgentBuilder {
		this._client = true;
		return this;
	}

	compute(): FindAgentBuilder {
		this._compute = true;
		return this;
	}

	prover(): FindAgentBuilder {
		this._prover = true;
		return this;
	}

	validator(): FindAgentBuilder {
		this._validator = true;
		return this;
	}

	env(env: string): FindAgentBuilder {
		if (this._all_env) {
			throw new Error('Cannot set env when all_envs is set');
		}

		this._env = env;
		return this;
	}

	all_envs(): FindAgentBuilder {
		if (this._env) {
			throw new Error('Cannot set all_envs when env is set');
		}

		this._all_env = true;
		return this;
	}

	labels(...labels: string[]): FindAgentBuilder {
		this._labels = labels;
		return this;
	}

	include_offline(): FindAgentBuilder {
		this._include_offline = true;
		return this;
	}

	local_pk(): FindAgentBuilder {
		this._local_pk = true;
		return this;
	}

	async find(): Promise<any> {
		return await this.api.findAgents('find', {
			mode: {
				client: this._client,
				compute: this._compute,
				prover: this._prover,
				validator: this._validator,
			},
			env: this._env,
			labels: this._labels,
			all: this._all_env,
			include_offline: this._include_offline,
			local_pk: this._local_pk,
		});
	}
}

// class Env {
// 	private env_api: string;

// 	constructor(api: string, env_id?: string) {
// 		this.env_api = `${base} / env / ${env_id ? env_id : 'default'}`;
// 	}

// 	action(): Action {
// 		return new Action(this.env_api);
// 	}

// 	execute(locator: string, ...inputs: string[]): ExecuteBuilder {
// 		return this.action().execute(this.env_api, locator, ...inputs);
// 	}
// }

// class Action {
// 	private action_api: string;

// 	constructor(base: string) {
// 		this.action_api = base + '/action/';
// 	}

// 	execute(locator: string, ...inputs: string[]): ExecuteBuilder {
// 		return new ExecuteBuilder(this.action_api, locator, inputs);
// 	}
// }

// class ExecuteBuilder {
// 	private url: string;
// 	private locator: string;
// 	private inputs: string[];
// 	private _private_key?: string;
// 	private cannon?: string;
// 	private priority_fee?: number;
// 	private fee_record?: string;

// 	constructor(url: string, locator: string, inputs: string[]) {
// 		this.locator = locator;
// 		this.inputs = inputs;
// 		this.url = `${url} / execute`;
// 	}

// 	set private_key(private_key: string) {
// 		this._private_key = private_key;
// 	}

// 	async call() {


// 	}
// }

const snops = new Snops('http://node.internal.monadic.us:1234');
// const res = await snops.agents().find().env('canary').validator().find();
const res = await snops.agents().list();
console.log(res);

// snops.env().execute('transfer_public', ...['committee.1', '1000u64']);
// snops.env().action().execute('transfer_public', ...['committee.1', '1000u64']).call();
