export class Snops {
	private api: SnopsApi;

	constructor(url: string) {
		this.api = new SnopsApi(url);
	}

	get agents(): Agents {
		return new Agents(this.api);
	}

	env(env_id?: string): Env {
		return new Env(this.api, env_id);
	}
}

class CustomError<T> extends Error {
	originalError: T;

	constructor(message: string, json_err: T) {
		super(message);
		this.name = 'CustomError';
		this.originalError = json_err;
	}
}

interface WithTargets<T> {
	nodes: string[] | string;
	data?: T;
}

interface ExternalNode {
	ip: string;
	External: {
		ports: {
			bft: string;
			node: string;
			rest: string;
		}
	}
}

interface InternalNode {
	Internal: {
		online: boolean;
		replicas?: number;
		key?: string;
		labels: string[];
		agent?: string;
		validators?: string[] | string;
		peers?: string[] | string;
		env: Record<string, string>;
	}
}

interface TopologyResponse {
	internal: Record<string, InternalNode>;
	external: Record<string, ExternalNode>;
}

class SnopsApi {
	private static API: string = '/api/v1/';
	private url: string;

	constructor(url: string) {
		this.url = url;
	}

	async fetch<B, T>(method: string, url: string, body?: B): Promise<T> {
		const full_url = `${this.url}${SnopsApi.API}${url}`;
		const res = await fetch(full_url, {
			method,
			body: body ? JSON.stringify(body) : null,
			headers: {
				'Content-Type': 'application/json'
			},
		});
		const rawBody = await res.text();
		const isErrorCode = res.status < 200 || res.status >= 300;
		let parsed: T;
		try {
			parsed = JSON.parse(rawBody);
		} catch (e) {
			throw new Error(`Failed to fetch '${full_url}' with status ${res.status} and body ${rawBody}: ${e}`);
		}

		if (isErrorCode) {
			throw new Error(JSON.stringify({
				parsed
			}, null, 2));
		};
		return parsed;
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

	async getAgentTps(id: string): Promise<number> {
		return this.get(`agents/${id}/tps`);
	}

	async findAgents(query: string, find: FindAgentsBody): Promise<AgentStatus[]> {
		console.debug(find);
		return this.post(`agents/find`, find);
	}

	async listEnvs(): Promise<string[]> {
		return await this.get('env/list');
	}

	async envTopology(env_id: string): Promise<TopologyResponse> {
		return await this.get<TopologyResponse>(`env/${env_id}/topology`);
	}

	async envResolvedTopology(env_id: string): Promise<any> {
		return await this.get(`env/${env_id}/topology/resolved`);
	}

	async envAgents(env_id: string): Promise<AgentStatus[]> {
		return await this.get(`env/${env_id}/agents`);
	}

	async envAgent(env_id: string, node_ty: string, node_key: string): Promise<AgentStatus> {
		return await this.get(`env/${env_id}/agents/${node_ty}/${node_key}`);
	}

	async envPrepare(env_id: string, prepare: any): Promise<any> {
		return this.post(`env/${env_id}/prepare`, prepare);
	}

	async envInfo(env_id: string): Promise<any> {
		return await this.get(`env/${env_id}/info`);
	}

	async executeAction(env_id: string, execute: any): Promise<Record<string, string>> {
		return this.post(`env/${env_id}/action/execute`, execute);
	}

	async onlineAction(env_id: string, nodes: string[] | string): Promise<Record<string, string>> {
		return this.post(`env/${env_id}/action/online`, { nodes });
	}

	async offlineAction(env_id: string, nodes: string[] | string): Promise<Record<string, string>> {
		return this.post(`env/${env_id}/action/offline`, { nodes });
	}

	async rebootAction(env_id: string, nodes: string[] | string): Promise<any> {
		return this.post(`env/${env_id}/action/reboot`, { nodes });
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

export class Agents {
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

	async tps(agent_id: string) {
		return await this.api.getAgentTps(agent_id);
	}

	get find() {
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

	with_labels(...labels: string[]): FindAgentBuilder {
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

	async call(): Promise<any> {
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

class Env {
	private api: SnopsApi;
	private env_id: string;

	constructor(api: SnopsApi, env_id?: string) {
		this.api = api;
		this.env_id = env_id || 'default';
	}

	get action(): Action {
		return new Action(this.api, this.env_id);
	}

	async list() {
		return this.api.listEnvs();
	}

	async topology() {
		return await this.api.envTopology(this.env_id);
	}

	async resolvedTopology() {
		return await this.api.envResolvedTopology(this.env_id);
	}

	async agents() {
		return await this.api.envAgents(this.env_id);
	}

	async agent(node_ty: string, node_key: string) {
		return await this.api.envAgent(this.env_id, node_ty, node_key);
	}

	async prepare(prepare: any) {
		return await this.api.envPrepare(this.env_id, prepare);
	}

	async info() {
		return await this.api.envInfo(this.env_id);
	}

	execute(locator: string, ...inputs: string[]): ExecuteBuilder {
		return this.action.execute(locator, ...inputs);
	}
}

class Action {
	private api: SnopsApi;
	private env_id: string;

	constructor(api: SnopsApi, env_id?: string) {
		this.api = api;
		this.env_id = env_id || 'default';
	}

	async online(nodes: string[] | string) {
		return await this.api.onlineAction(this.env_id, nodes);
	}

	async offline(nodes: string[] | string) {
		return await this.api.offlineAction(this.env_id, nodes);
	}

	async reboot(nodes: string[] | string) {
		return await this.api.rebootAction(this.env_id, nodes);
	}

	execute(locator: string, ...inputs: string[]): ExecuteBuilder {
		return new ExecuteBuilder(this.api, this.env_id, locator, inputs);
	}
}

class ExecuteBuilder {
	private api: SnopsApi;
	private env_id: string;

	private program?: string;
	private fn: string;
	private inputs: string[];
	private _private_key?: string;
	private _cannon?: string;
	private _priority_fee?: number;
	private _fee_record?: string;

	constructor(api: SnopsApi, env_id: string, locator: string, inputs: string[]) {
		this.api = api;
		this.env_id = env_id;
		this.inputs = inputs;

		const parts = locator.split('/');
		if (parts.length === 1) {
			this.fn = parts[0];
		} else if (parts.length === 2) {
			this.program = parts[0];
			this.fn = parts[1];
		} else {
			throw new Error(`Invalid locator ${locator}`);
		}
	}

	private_key(private_key: string): ExecuteBuilder {
		this._private_key = private_key;
		return this;
	}

	cannon(cannon: string): ExecuteBuilder {
		this._cannon = cannon;
		return this;
	}

	priority_fee(priority_fee: number): ExecuteBuilder {
		this._priority_fee = priority_fee;
		return this;
	}

	fee_record(fee_record: string): ExecuteBuilder {
		this._fee_record = fee_record;
		return this;
	}

	async call() {
		return await this.api.executeAction(this.env_id, {
			private_key: this._private_key,
			cannon: this._cannon,
			priority_fee: this._priority_fee,
			fee_record: this._fee_record,
			program: this.program,
			function: this.fn,
			inputs: this.inputs,
		});
	}
}
