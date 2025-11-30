/* tslint:disable */
/* eslint-disable */

export class DemoApp {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Make an isolated GET request (new circuit each time)
   */
  getIsolated(url: string): Promise<any>;
  /**
   * Get circuit relay information
   */
  getCircuitRelays(): Promise<any>;
  /**
   * Set a callback function for status updates
   */
  setStatusCallback(callback: Function): void;
  /**
   * Trigger a circuit update
   */
  triggerCircuitUpdate(): Promise<any>;
  /**
   * Make a GET request using the persistent circuit
   */
  get(url: string): Promise<any>;
  constructor();
  /**
   * Open the TorClient and wait for circuit
   */
  open(): Promise<any>;
  /**
   * Close the TorClient
   */
  close(): Promise<any>;
}

export class JsCircuitRelay {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  readonly fingerprint: string;
  readonly role: string;
  readonly address: string;
  readonly nickname: string;
}

export class JsCircuitStatus {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  readonly is_healthy: boolean;
  readonly ready_circuits: number;
  readonly total_circuits: number;
  readonly failed_circuits: number;
  readonly creating_circuits: number;
  readonly has_ready_circuits: boolean;
}

export class JsHttpResponse {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  json(): any;
  text(): string;
  readonly url: string;
  readonly body: Uint8Array;
  readonly status: number;
  readonly headers: any;
}

export class TorClient {
  free(): void;
  [Symbol.dispose](): void;
  close_rust(): Promise<void>;
  fetch_rust(url: string): Promise<JsHttpResponse>;
  /**
   * Make a one-time fetch request (static method)
   */
  static fetchOneTime(snowflake_url: string, url: string, connection_timeout?: number | null, circuit_timeout?: number | null): Promise<any>;
  /**
   * Update the circuit
   */
  updateCircuit(deadline_ms: number): Promise<any>;
  /**
   * Wait for circuit to be ready
   */
  waitForCircuit(): Promise<any>;
  /**
   * Get circuit relays info
   */
  getCircuitRelays(): Promise<any>;
  /**
   * Get circuit status
   */
  getCircuitStatus(): Promise<any>;
  static fetch_one_time_rust(snowflake_url: string, url: string, connection_timeout?: bigint | null, circuit_timeout?: bigint | null): Promise<JsHttpResponse>;
  update_circuit_rust(deadline_ms: bigint): Promise<void>;
  wait_for_circuit_rust(): Promise<void>;
  get_circuit_relays_rust(): Promise<JsCircuitRelay[] | undefined>;
  /**
   * Get circuit status string
   */
  getCircuitStatusString(): Promise<any>;
  get_circuit_status_string_rust(): Promise<string>;
  constructor(options: TorClientOptions);
  /**
   * Close the Tor client
   */
  close(): Promise<any>;
  /**
   * Make a fetch request through Tor
   */
  fetch(url: string): Promise<any>;
  static create(options: TorClientOptions): Promise<TorClient>;
}

export class TorClientOptions {
  free(): void;
  [Symbol.dispose](): void;
  withCircuitTimeout(timeout: number): TorClientOptions;
  withBridgeFingerprint(fingerprint: string): TorClientOptions;
  withConnectionTimeout(timeout: number): TorClientOptions;
  withCreateCircuitEarly(create_early: boolean): TorClientOptions;
  withCircuitUpdateAdvance(advance: number): TorClientOptions;
  withCircuitUpdateInterval(interval?: number | null): TorClientOptions;
  /**
   * Create options for Snowflake bridge (default)
   */
  constructor(snowflake_url: string);
  /**
   * Create options for WebTunnel bridge
   */
  static webtunnel(url: string, fingerprint: string): TorClientOptions;
}

/**
 * Initialize the WASM module
 */
export function init(): void;

/**
 * Initialize logging when module loads
 */
export function main(): void;

/**
 * Enable or disable debug-level logging
 */
export function setDebugEnabled(enabled: boolean): void;

/**
 * Set the log callback function for receiving tracing logs in JavaScript
 */
export function setLogCallback(callback: Function): void;

/**
 * Test function for WASM
 */
export function test_wasm(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_demoapp_free: (a: number, b: number) => void;
  readonly demoapp_close: (a: number) => any;
  readonly demoapp_get: (a: number, b: number, c: number) => any;
  readonly demoapp_getCircuitRelays: (a: number) => any;
  readonly demoapp_getIsolated: (a: number, b: number, c: number) => any;
  readonly demoapp_new: () => [number, number, number];
  readonly demoapp_open: (a: number) => any;
  readonly demoapp_setStatusCallback: (a: number, b: any) => void;
  readonly demoapp_triggerCircuitUpdate: (a: number) => any;
  readonly main: () => void;
  readonly __wbg_jscircuitrelay_free: (a: number, b: number) => void;
  readonly __wbg_jscircuitstatus_free: (a: number, b: number) => void;
  readonly __wbg_jshttpresponse_free: (a: number, b: number) => void;
  readonly __wbg_torclient_free: (a: number, b: number) => void;
  readonly __wbg_torclientoptions_free: (a: number, b: number) => void;
  readonly init: () => void;
  readonly jscircuitrelay_address: (a: number) => [number, number];
  readonly jscircuitrelay_fingerprint: (a: number) => [number, number];
  readonly jscircuitrelay_nickname: (a: number) => [number, number];
  readonly jscircuitrelay_role: (a: number) => [number, number];
  readonly jscircuitstatus_creating_circuits: (a: number) => number;
  readonly jscircuitstatus_failed_circuits: (a: number) => number;
  readonly jscircuitstatus_has_ready_circuits: (a: number) => number;
  readonly jscircuitstatus_is_healthy: (a: number) => number;
  readonly jscircuitstatus_ready_circuits: (a: number) => number;
  readonly jscircuitstatus_total_circuits: (a: number) => number;
  readonly jshttpresponse_body: (a: number) => [number, number];
  readonly jshttpresponse_headers: (a: number) => any;
  readonly jshttpresponse_json: (a: number) => [number, number, number];
  readonly jshttpresponse_status: (a: number) => number;
  readonly jshttpresponse_text: (a: number) => [number, number, number, number];
  readonly jshttpresponse_url: (a: number) => [number, number];
  readonly setDebugEnabled: (a: number) => void;
  readonly test_wasm: () => [number, number];
  readonly torclient_close: (a: number) => any;
  readonly torclient_close_rust: (a: number) => any;
  readonly torclient_create: (a: number) => any;
  readonly torclient_fetch: (a: number, b: number, c: number) => any;
  readonly torclient_fetchOneTime: (a: number, b: number, c: number, d: number, e: number, f: number) => any;
  readonly torclient_fetch_one_time_rust: (a: number, b: number, c: number, d: number, e: number, f: bigint, g: number, h: bigint) => any;
  readonly torclient_fetch_rust: (a: number, b: number, c: number) => any;
  readonly torclient_getCircuitRelays: (a: number) => any;
  readonly torclient_getCircuitStatus: (a: number) => any;
  readonly torclient_getCircuitStatusString: (a: number) => any;
  readonly torclient_get_circuit_relays_rust: (a: number) => any;
  readonly torclient_get_circuit_status_string_rust: (a: number) => any;
  readonly torclient_new: (a: number) => any;
  readonly torclient_updateCircuit: (a: number, b: number) => any;
  readonly torclient_update_circuit_rust: (a: number, b: bigint) => any;
  readonly torclient_waitForCircuit: (a: number) => any;
  readonly torclient_wait_for_circuit_rust: (a: number) => any;
  readonly torclientoptions_new: (a: number, b: number) => number;
  readonly torclientoptions_webtunnel: (a: number, b: number, c: number, d: number) => number;
  readonly torclientoptions_withBridgeFingerprint: (a: number, b: number, c: number) => number;
  readonly torclientoptions_withCircuitTimeout: (a: number, b: number) => number;
  readonly torclientoptions_withCircuitUpdateAdvance: (a: number, b: number) => number;
  readonly torclientoptions_withCircuitUpdateInterval: (a: number, b: number) => number;
  readonly torclientoptions_withConnectionTimeout: (a: number, b: number) => number;
  readonly torclientoptions_withCreateCircuitEarly: (a: number, b: number) => number;
  readonly setLogCallback: (a: any) => void;
  readonly wasm_bindgen__convert__closures_____invoke__h15fcc68dda9f98b6: (a: number, b: number) => void;
  readonly wasm_bindgen__closure__destroy__h03f628fb5c1af182: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__h32d5e12558544916: (a: number, b: number, c: any) => void;
  readonly wasm_bindgen__closure__destroy__hf76ded83d5f84246: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__h437f43f99c40f53c: (a: number, b: number, c: any) => void;
  readonly wasm_bindgen__closure__destroy__h00070d11570d9b01: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__hd0509b06bbeda2ff: (a: number, b: number, c: any, d: any) => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __externref_drop_slice: (a: number, b: number) => void;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
