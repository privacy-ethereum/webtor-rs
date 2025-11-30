let wasm;

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
}

const CLOSURE_DTORS = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(state => state.dtor(state.a, state.b));

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches && builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function makeMutClosure(arg0, arg1, dtor, f) {
    const state = { a: arg0, b: arg1, cnt: 1, dtor };
    const real = (...args) => {

        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        const a = state.a;
        state.a = 0;
        try {
            return f(a, state.b, ...args);
        } finally {
            state.a = a;
            real._wbg_cb_unref();
        }
    };
    real._wbg_cb_unref = () => {
        if (--state.cnt === 0) {
            state.dtor(state.a, state.b);
            state.a = 0;
            CLOSURE_DTORS.unregister(state);
        }
    };
    CLOSURE_DTORS.register(real, state, state);
    return real;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    }
}

let WASM_VECTOR_LEN = 0;

function wasm_bindgen__convert__closures_____invoke__h15fcc68dda9f98b6(arg0, arg1) {
    wasm.wasm_bindgen__convert__closures_____invoke__h15fcc68dda9f98b6(arg0, arg1);
}

function wasm_bindgen__convert__closures_____invoke__h32d5e12558544916(arg0, arg1, arg2) {
    wasm.wasm_bindgen__convert__closures_____invoke__h32d5e12558544916(arg0, arg1, arg2);
}

function wasm_bindgen__convert__closures_____invoke__h437f43f99c40f53c(arg0, arg1, arg2) {
    wasm.wasm_bindgen__convert__closures_____invoke__h437f43f99c40f53c(arg0, arg1, arg2);
}

function wasm_bindgen__convert__closures_____invoke__hd0509b06bbeda2ff(arg0, arg1, arg2, arg3) {
    wasm.wasm_bindgen__convert__closures_____invoke__hd0509b06bbeda2ff(arg0, arg1, arg2, arg3);
}

const __wbindgen_enum_BinaryType = ["blob", "arraybuffer"];

const __wbindgen_enum_RequestMode = ["same-origin", "no-cors", "cors", "navigate"];

const DemoAppFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_demoapp_free(ptr >>> 0, 1));

const JsCircuitRelayFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_jscircuitrelay_free(ptr >>> 0, 1));

const JsCircuitStatusFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_jscircuitstatus_free(ptr >>> 0, 1));

const JsHttpResponseFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_jshttpresponse_free(ptr >>> 0, 1));

const TorClientFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_torclient_free(ptr >>> 0, 1));

const TorClientOptionsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_torclientoptions_free(ptr >>> 0, 1));

/**
 * Main demo application - simplified API for JavaScript
 */
export class DemoApp {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        DemoAppFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_demoapp_free(ptr, 0);
    }
    /**
     * Make an isolated GET request (new circuit each time)
     * @param {string} url
     * @returns {Promise<any>}
     */
    getIsolated(url) {
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.demoapp_getIsolated(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * Get circuit relay information
     * @returns {Promise<any>}
     */
    getCircuitRelays() {
        const ret = wasm.demoapp_getCircuitRelays(this.__wbg_ptr);
        return ret;
    }
    /**
     * Set a callback function for status updates
     * @param {Function} callback
     */
    setStatusCallback(callback) {
        wasm.demoapp_setStatusCallback(this.__wbg_ptr, callback);
    }
    /**
     * Trigger a circuit update
     * @returns {Promise<any>}
     */
    triggerCircuitUpdate() {
        const ret = wasm.demoapp_triggerCircuitUpdate(this.__wbg_ptr);
        return ret;
    }
    /**
     * Make a GET request using the persistent circuit
     * @param {string} url
     * @returns {Promise<any>}
     */
    get(url) {
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.demoapp_get(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    constructor() {
        const ret = wasm.demoapp_new();
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        DemoAppFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Open the TorClient and wait for circuit
     * @returns {Promise<any>}
     */
    open() {
        const ret = wasm.demoapp_open(this.__wbg_ptr);
        return ret;
    }
    /**
     * Close the TorClient
     * @returns {Promise<any>}
     */
    close() {
        const ret = wasm.demoapp_close(this.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) DemoApp.prototype[Symbol.dispose] = DemoApp.prototype.free;

/**
 * JavaScript-friendly circuit relay info
 */
export class JsCircuitRelay {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(JsCircuitRelay.prototype);
        obj.__wbg_ptr = ptr;
        JsCircuitRelayFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        JsCircuitRelayFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_jscircuitrelay_free(ptr, 0);
    }
    /**
     * @returns {string}
     */
    get fingerprint() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.jscircuitrelay_fingerprint(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    get role() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.jscircuitrelay_role(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    get address() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.jscircuitrelay_address(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    get nickname() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.jscircuitrelay_nickname(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) JsCircuitRelay.prototype[Symbol.dispose] = JsCircuitRelay.prototype.free;

/**
 * JavaScript-friendly circuit status
 */
export class JsCircuitStatus {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(JsCircuitStatus.prototype);
        obj.__wbg_ptr = ptr;
        JsCircuitStatusFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        JsCircuitStatusFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_jscircuitstatus_free(ptr, 0);
    }
    /**
     * @returns {boolean}
     */
    get is_healthy() {
        const ret = wasm.jscircuitstatus_is_healthy(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    get ready_circuits() {
        const ret = wasm.jscircuitstatus_ready_circuits(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get total_circuits() {
        const ret = wasm.jscircuitstatus_total_circuits(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get failed_circuits() {
        const ret = wasm.jscircuitstatus_failed_circuits(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get creating_circuits() {
        const ret = wasm.jscircuitstatus_creating_circuits(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {boolean}
     */
    get has_ready_circuits() {
        const ret = wasm.jscircuitstatus_has_ready_circuits(this.__wbg_ptr);
        return ret !== 0;
    }
}
if (Symbol.dispose) JsCircuitStatus.prototype[Symbol.dispose] = JsCircuitStatus.prototype.free;

/**
 * JavaScript-friendly HTTP response
 */
export class JsHttpResponse {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(JsHttpResponse.prototype);
        obj.__wbg_ptr = ptr;
        JsHttpResponseFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        JsHttpResponseFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_jshttpresponse_free(ptr, 0);
    }
    /**
     * @returns {string}
     */
    get url() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.jshttpresponse_url(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    get body() {
        const ret = wasm.jshttpresponse_body(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {any}
     */
    json() {
        const ret = wasm.jshttpresponse_json(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * @returns {string}
     */
    text() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.jshttpresponse_text(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * @returns {number}
     */
    get status() {
        const ret = wasm.jshttpresponse_status(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {any}
     */
    get headers() {
        const ret = wasm.jshttpresponse_headers(this.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) JsHttpResponse.prototype[Symbol.dispose] = JsHttpResponse.prototype.free;

/**
 * JavaScript-friendly TorClient
 */
export class TorClient {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(TorClient.prototype);
        obj.__wbg_ptr = ptr;
        TorClientFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TorClientFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_torclient_free(ptr, 0);
    }
    /**
     * @returns {Promise<void>}
     */
    close_rust() {
        const ret = wasm.torclient_close_rust(this.__wbg_ptr);
        return ret;
    }
    /**
     * @param {string} url
     * @returns {Promise<JsHttpResponse>}
     */
    fetch_rust(url) {
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.torclient_fetch_rust(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * Make a one-time fetch request (static method)
     * @param {string} snowflake_url
     * @param {string} url
     * @param {number | null} [connection_timeout]
     * @param {number | null} [circuit_timeout]
     * @returns {Promise<any>}
     */
    static fetchOneTime(snowflake_url, url, connection_timeout, circuit_timeout) {
        const ptr0 = passStringToWasm0(snowflake_url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.torclient_fetchOneTime(ptr0, len0, ptr1, len1, isLikeNone(connection_timeout) ? 0x100000001 : (connection_timeout) >>> 0, isLikeNone(circuit_timeout) ? 0x100000001 : (circuit_timeout) >>> 0);
        return ret;
    }
    /**
     * Update the circuit
     * @param {number} deadline_ms
     * @returns {Promise<any>}
     */
    updateCircuit(deadline_ms) {
        const ret = wasm.torclient_updateCircuit(this.__wbg_ptr, deadline_ms);
        return ret;
    }
    /**
     * Wait for circuit to be ready
     * @returns {Promise<any>}
     */
    waitForCircuit() {
        const ret = wasm.torclient_waitForCircuit(this.__wbg_ptr);
        return ret;
    }
    /**
     * Get circuit relays info
     * @returns {Promise<any>}
     */
    getCircuitRelays() {
        const ret = wasm.torclient_getCircuitRelays(this.__wbg_ptr);
        return ret;
    }
    /**
     * Get circuit status
     * @returns {Promise<any>}
     */
    getCircuitStatus() {
        const ret = wasm.torclient_getCircuitStatus(this.__wbg_ptr);
        return ret;
    }
    /**
     * @param {string} snowflake_url
     * @param {string} url
     * @param {bigint | null} [connection_timeout]
     * @param {bigint | null} [circuit_timeout]
     * @returns {Promise<JsHttpResponse>}
     */
    static fetch_one_time_rust(snowflake_url, url, connection_timeout, circuit_timeout) {
        const ptr0 = passStringToWasm0(snowflake_url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.torclient_fetch_one_time_rust(ptr0, len0, ptr1, len1, !isLikeNone(connection_timeout), isLikeNone(connection_timeout) ? BigInt(0) : connection_timeout, !isLikeNone(circuit_timeout), isLikeNone(circuit_timeout) ? BigInt(0) : circuit_timeout);
        return ret;
    }
    /**
     * @param {bigint} deadline_ms
     * @returns {Promise<void>}
     */
    update_circuit_rust(deadline_ms) {
        const ret = wasm.torclient_update_circuit_rust(this.__wbg_ptr, deadline_ms);
        return ret;
    }
    /**
     * @returns {Promise<void>}
     */
    wait_for_circuit_rust() {
        const ret = wasm.torclient_wait_for_circuit_rust(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {Promise<JsCircuitRelay[] | undefined>}
     */
    get_circuit_relays_rust() {
        const ret = wasm.torclient_get_circuit_relays_rust(this.__wbg_ptr);
        return ret;
    }
    /**
     * Get circuit status string
     * @returns {Promise<any>}
     */
    getCircuitStatusString() {
        const ret = wasm.torclient_getCircuitStatusString(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {Promise<string>}
     */
    get_circuit_status_string_rust() {
        const ret = wasm.torclient_get_circuit_status_string_rust(this.__wbg_ptr);
        return ret;
    }
    /**
     * @param {TorClientOptions} options
     */
    constructor(options) {
        _assertClass(options, TorClientOptions);
        var ptr0 = options.__destroy_into_raw();
        const ret = wasm.torclient_new(ptr0);
        return ret;
    }
    /**
     * Close the Tor client
     * @returns {Promise<any>}
     */
    close() {
        const ret = wasm.torclient_close(this.__wbg_ptr);
        return ret;
    }
    /**
     * Make a fetch request through Tor
     * @param {string} url
     * @returns {Promise<any>}
     */
    fetch(url) {
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.torclient_fetch(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * @param {TorClientOptions} options
     * @returns {Promise<TorClient>}
     */
    static create(options) {
        _assertClass(options, TorClientOptions);
        var ptr0 = options.__destroy_into_raw();
        const ret = wasm.torclient_create(ptr0);
        return ret;
    }
}
if (Symbol.dispose) TorClient.prototype[Symbol.dispose] = TorClient.prototype.free;

/**
 * JavaScript-friendly options for TorClient
 */
export class TorClientOptions {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(TorClientOptions.prototype);
        obj.__wbg_ptr = ptr;
        TorClientOptionsFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TorClientOptionsFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_torclientoptions_free(ptr, 0);
    }
    /**
     * @param {number} timeout
     * @returns {TorClientOptions}
     */
    withCircuitTimeout(timeout) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.torclientoptions_withCircuitTimeout(ptr, timeout);
        return TorClientOptions.__wrap(ret);
    }
    /**
     * @param {string} fingerprint
     * @returns {TorClientOptions}
     */
    withBridgeFingerprint(fingerprint) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(fingerprint, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.torclientoptions_withBridgeFingerprint(ptr, ptr0, len0);
        return TorClientOptions.__wrap(ret);
    }
    /**
     * @param {number} timeout
     * @returns {TorClientOptions}
     */
    withConnectionTimeout(timeout) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.torclientoptions_withConnectionTimeout(ptr, timeout);
        return TorClientOptions.__wrap(ret);
    }
    /**
     * @param {boolean} create_early
     * @returns {TorClientOptions}
     */
    withCreateCircuitEarly(create_early) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.torclientoptions_withCreateCircuitEarly(ptr, create_early);
        return TorClientOptions.__wrap(ret);
    }
    /**
     * @param {number} advance
     * @returns {TorClientOptions}
     */
    withCircuitUpdateAdvance(advance) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.torclientoptions_withCircuitUpdateAdvance(ptr, advance);
        return TorClientOptions.__wrap(ret);
    }
    /**
     * @param {number | null} [interval]
     * @returns {TorClientOptions}
     */
    withCircuitUpdateInterval(interval) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.torclientoptions_withCircuitUpdateInterval(ptr, isLikeNone(interval) ? 0x100000001 : (interval) >>> 0);
        return TorClientOptions.__wrap(ret);
    }
    /**
     * Create options for Snowflake bridge (default)
     * @param {string} snowflake_url
     */
    constructor(snowflake_url) {
        const ptr0 = passStringToWasm0(snowflake_url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.torclientoptions_new(ptr0, len0);
        this.__wbg_ptr = ret >>> 0;
        TorClientOptionsFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Create options for WebTunnel bridge
     * @param {string} url
     * @param {string} fingerprint
     * @returns {TorClientOptions}
     */
    static webtunnel(url, fingerprint) {
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(fingerprint, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.torclientoptions_webtunnel(ptr0, len0, ptr1, len1);
        return TorClientOptions.__wrap(ret);
    }
}
if (Symbol.dispose) TorClientOptions.prototype[Symbol.dispose] = TorClientOptions.prototype.free;

/**
 * Initialize the WASM module
 */
export function init() {
    wasm.init();
}

/**
 * Initialize logging when module loads
 */
export function main() {
    wasm.main();
}

/**
 * Enable or disable debug-level logging
 * @param {boolean} enabled
 */
export function setDebugEnabled(enabled) {
    wasm.setDebugEnabled(enabled);
}

/**
 * Set the log callback function for receiving tracing logs in JavaScript
 * @param {Function} callback
 */
export function setLogCallback(callback) {
    wasm.setLogCallback(callback);
}

/**
 * Test function for WASM
 * @returns {string}
 */
export function test_wasm() {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.test_wasm();
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}

const EXPECTED_RESPONSE_TYPES = new Set(['basic', 'cors', 'default']);

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && EXPECTED_RESPONSE_TYPES.has(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else {
                    throw e;
                }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }
}

function __wbg_get_imports() {
    const imports = {};
    imports.wbg = {};
    imports.wbg.__wbg_Error_52673b7de5a0ca89 = function(arg0, arg1) {
        const ret = Error(getStringFromWasm0(arg0, arg1));
        return ret;
    };
    imports.wbg.__wbg___wbindgen_boolean_get_dea25b33882b895b = function(arg0) {
        const v = arg0;
        const ret = typeof(v) === 'boolean' ? v : undefined;
        return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
    };
    imports.wbg.__wbg___wbindgen_debug_string_adfb662ae34724b6 = function(arg0, arg1) {
        const ret = debugString(arg1);
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg___wbindgen_is_function_8d400b8b1af978cd = function(arg0) {
        const ret = typeof(arg0) === 'function';
        return ret;
    };
    imports.wbg.__wbg___wbindgen_is_object_ce774f3490692386 = function(arg0) {
        const val = arg0;
        const ret = typeof(val) === 'object' && val !== null;
        return ret;
    };
    imports.wbg.__wbg___wbindgen_is_string_704ef9c8fc131030 = function(arg0) {
        const ret = typeof(arg0) === 'string';
        return ret;
    };
    imports.wbg.__wbg___wbindgen_is_undefined_f6b95eab589e0269 = function(arg0) {
        const ret = arg0 === undefined;
        return ret;
    };
    imports.wbg.__wbg___wbindgen_string_get_a2a31e16edf96e42 = function(arg0, arg1) {
        const obj = arg1;
        const ret = typeof(obj) === 'string' ? obj : undefined;
        var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg___wbindgen_throw_dd24417ed36fc46e = function(arg0, arg1) {
        throw new Error(getStringFromWasm0(arg0, arg1));
    };
    imports.wbg.__wbg__wbg_cb_unref_87dfb5aaa0cbcea7 = function(arg0) {
        arg0._wbg_cb_unref();
    };
    imports.wbg.__wbg_arrayBuffer_c04af4fce566092d = function() { return handleError(function (arg0) {
        const ret = arg0.arrayBuffer();
        return ret;
    }, arguments) };
    imports.wbg.__wbg_buffer_6cb2fecb1f253d71 = function(arg0) {
        const ret = arg0.buffer;
        return ret;
    };
    imports.wbg.__wbg_call_3020136f7a2d6e44 = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = arg0.call(arg1, arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_call_78f94eb02ec7f9b2 = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
        const ret = arg0.call(arg1, arg2, arg3, arg4);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_call_abb4ff46ce38be40 = function() { return handleError(function (arg0, arg1) {
        const ret = arg0.call(arg1);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_clearInterval_45ac4607741420fd = function(arg0, arg1) {
        arg0.clearInterval(arg1);
    };
    imports.wbg.__wbg_close_1db3952de1b5b1cf = function() { return handleError(function (arg0) {
        arg0.close();
    }, arguments) };
    imports.wbg.__wbg_code_85a811fe6ca962be = function(arg0) {
        const ret = arg0.code;
        return ret;
    };
    imports.wbg.__wbg_crypto_574e78ad8b13b65f = function(arg0) {
        const ret = arg0.crypto;
        return ret;
    };
    imports.wbg.__wbg_crypto_59726e04573101a0 = function() { return handleError(function (arg0) {
        const ret = arg0.crypto;
        return ret;
    }, arguments) };
    imports.wbg.__wbg_data_8bf4ae669a78a688 = function(arg0) {
        const ret = arg0.data;
        return ret;
    };
    imports.wbg.__wbg_decrypt_f10fd2439f5feff7 = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        const ret = arg0.decrypt(arg1, arg2, arg3);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_deriveBits_c2caf1b3524ca3df = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        const ret = arg0.deriveBits(arg1, arg2, arg3 >>> 0);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_digest_39377c08dd5c9faf = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        const ret = arg0.digest(getStringFromWasm0(arg1, arg2), arg3);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_encodeURIComponent_fe8578929b74aa6c = function(arg0, arg1) {
        const ret = encodeURIComponent(getStringFromWasm0(arg0, arg1));
        return ret;
    };
    imports.wbg.__wbg_encrypt_720a75124a6ce3e9 = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        const ret = arg0.encrypt(arg1, arg2, arg3);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_error_3c7d958458bf649b = function(arg0, arg1) {
        var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
        wasm.__wbindgen_free(arg0, arg1 * 4, 4);
        console.error(...v0);
    };
    imports.wbg.__wbg_error_7534b8e9a36f1ab4 = function(arg0, arg1) {
        let deferred0_0;
        let deferred0_1;
        try {
            deferred0_0 = arg0;
            deferred0_1 = arg1;
            console.error(getStringFromWasm0(arg0, arg1));
        } finally {
            wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
        }
    };
    imports.wbg.__wbg_exportKey_fd2f753afcdf2526 = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        const ret = arg0.exportKey(getStringFromWasm0(arg1, arg2), arg3);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_fetch_8119fbf8d0e4f4d1 = function(arg0, arg1) {
        const ret = arg0.fetch(arg1);
        return ret;
    };
    imports.wbg.__wbg_generateKey_1575e199b1d2d83d = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        const ret = arg0.generateKey(arg1, arg2 !== 0, arg3);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_getRandomValues_1c61fac11405ffdc = function() { return handleError(function (arg0, arg1) {
        globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
    }, arguments) };
    imports.wbg.__wbg_getRandomValues_9b655bdd369112f2 = function() { return handleError(function (arg0, arg1) {
        globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
    }, arguments) };
    imports.wbg.__wbg_getRandomValues_b8f5dbd5f3995a9e = function() { return handleError(function (arg0, arg1) {
        arg0.getRandomValues(arg1);
    }, arguments) };
    imports.wbg.__wbg_get_af9dab7e9603ea93 = function() { return handleError(function (arg0, arg1) {
        const ret = Reflect.get(arg0, arg1);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_importKey_d2e413c2af4484d1 = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
        const ret = arg0.importKey(getStringFromWasm0(arg1, arg2), arg3, arg4, arg5 !== 0, arg6);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_instanceof_ArrayBuffer_f3320d2419cd0355 = function(arg0) {
        let result;
        try {
            result = arg0 instanceof ArrayBuffer;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_instanceof_Response_cd74d1c2ac92cb0b = function(arg0) {
        let result;
        try {
            result = arg0 instanceof Response;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_instanceof_Window_b5cf7783caa68180 = function(arg0) {
        let result;
        try {
            result = arg0 instanceof Window;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_jscircuitrelay_new = function(arg0) {
        const ret = JsCircuitRelay.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_jscircuitstatus_new = function(arg0) {
        const ret = JsCircuitStatus.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_jshttpresponse_new = function(arg0) {
        const ret = JsHttpResponse.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_length_22ac23eaec9d8053 = function(arg0) {
        const ret = arg0.length;
        return ret;
    };
    imports.wbg.__wbg_log_0cc1b7768397bcfe = function(arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7) {
        let deferred0_0;
        let deferred0_1;
        try {
            deferred0_0 = arg0;
            deferred0_1 = arg1;
            console.log(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3), getStringFromWasm0(arg4, arg5), getStringFromWasm0(arg6, arg7));
        } finally {
            wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
        }
    };
    imports.wbg.__wbg_log_1d990106d99dacb7 = function(arg0) {
        console.log(arg0);
    };
    imports.wbg.__wbg_log_c3d56bb0009edd6a = function(arg0, arg1) {
        var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
        wasm.__wbindgen_free(arg0, arg1 * 4, 4);
        console.log(...v0);
    };
    imports.wbg.__wbg_log_cb9e190acc5753fb = function(arg0, arg1) {
        let deferred0_0;
        let deferred0_1;
        try {
            deferred0_0 = arg0;
            deferred0_1 = arg1;
            console.log(getStringFromWasm0(arg0, arg1));
        } finally {
            wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
        }
    };
    imports.wbg.__wbg_mark_7438147ce31e9d4b = function(arg0, arg1) {
        performance.mark(getStringFromWasm0(arg0, arg1));
    };
    imports.wbg.__wbg_measure_fb7825c11612c823 = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        let deferred0_0;
        let deferred0_1;
        let deferred1_0;
        let deferred1_1;
        try {
            deferred0_0 = arg0;
            deferred0_1 = arg1;
            deferred1_0 = arg2;
            deferred1_1 = arg3;
            performance.measure(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3));
        } finally {
            wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }, arguments) };
    imports.wbg.__wbg_message_0ff7f09380783844 = function(arg0, arg1) {
        const ret = arg1.message;
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg_msCrypto_a61aeb35a24c1329 = function(arg0) {
        const ret = arg0.msCrypto;
        return ret;
    };
    imports.wbg.__wbg_new_1ba21ce319a06297 = function() {
        const ret = new Object();
        return ret;
    };
    imports.wbg.__wbg_new_25f239778d6112b9 = function() {
        const ret = new Array();
        return ret;
    };
    imports.wbg.__wbg_new_6421f6084cc5bc5a = function(arg0) {
        const ret = new Uint8Array(arg0);
        return ret;
    };
    imports.wbg.__wbg_new_7c30d1f874652e62 = function() { return handleError(function (arg0, arg1) {
        const ret = new WebSocket(getStringFromWasm0(arg0, arg1));
        return ret;
    }, arguments) };
    imports.wbg.__wbg_new_8a6f238a6ece86ea = function() {
        const ret = new Error();
        return ret;
    };
    imports.wbg.__wbg_new_b546ae120718850e = function() {
        const ret = new Map();
        return ret;
    };
    imports.wbg.__wbg_new_ff12d2b041fb48f1 = function(arg0, arg1) {
        try {
            var state0 = {a: arg0, b: arg1};
            var cb0 = (arg0, arg1) => {
                const a = state0.a;
                state0.a = 0;
                try {
                    return wasm_bindgen__convert__closures_____invoke__hd0509b06bbeda2ff(a, state0.b, arg0, arg1);
                } finally {
                    state0.a = a;
                }
            };
            const ret = new Promise(cb0);
            return ret;
        } finally {
            state0.a = state0.b = 0;
        }
    };
    imports.wbg.__wbg_new_from_slice_f9c22b9153b26992 = function(arg0, arg1) {
        const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
        return ret;
    };
    imports.wbg.__wbg_new_no_args_cb138f77cf6151ee = function(arg0, arg1) {
        const ret = new Function(getStringFromWasm0(arg0, arg1));
        return ret;
    };
    imports.wbg.__wbg_new_with_length_aa5eaf41d35235e5 = function(arg0) {
        const ret = new Uint8Array(arg0 >>> 0);
        return ret;
    };
    imports.wbg.__wbg_new_with_str_and_init_c5748f76f5108934 = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = new Request(getStringFromWasm0(arg0, arg1), arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_node_905d3e251edff8a2 = function(arg0) {
        const ret = arg0.node;
        return ret;
    };
    imports.wbg.__wbg_now_69d776cd24f5215b = function() {
        const ret = Date.now();
        return ret;
    };
    imports.wbg.__wbg_now_8cf15d6e317793e1 = function(arg0) {
        const ret = arg0.now();
        return ret;
    };
    imports.wbg.__wbg_ok_dd98ecb60d721e20 = function(arg0) {
        const ret = arg0.ok;
        return ret;
    };
    imports.wbg.__wbg_performance_c77a440eff2efd9b = function(arg0) {
        const ret = arg0.performance;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_process_dc0fbacc7c1c06f7 = function(arg0) {
        const ret = arg0.process;
        return ret;
    };
    imports.wbg.__wbg_prototypesetcall_dfe9b766cdc1f1fd = function(arg0, arg1, arg2) {
        Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
    };
    imports.wbg.__wbg_push_7d9be8f38fc13975 = function(arg0, arg1) {
        const ret = arg0.push(arg1);
        return ret;
    };
    imports.wbg.__wbg_queueMicrotask_9b549dfce8865860 = function(arg0) {
        const ret = arg0.queueMicrotask;
        return ret;
    };
    imports.wbg.__wbg_queueMicrotask_fca69f5bfad613a5 = function(arg0) {
        queueMicrotask(arg0);
    };
    imports.wbg.__wbg_randomFillSync_ac0988aba3254290 = function() { return handleError(function (arg0, arg1) {
        arg0.randomFillSync(arg1);
    }, arguments) };
    imports.wbg.__wbg_reason_d4eb9e40592438c2 = function(arg0, arg1) {
        const ret = arg1.reason;
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg_require_60cc747a6bc5215a = function() { return handleError(function () {
        const ret = module.require;
        return ret;
    }, arguments) };
    imports.wbg.__wbg_resolve_fd5bfbaa4ce36e1e = function(arg0) {
        const ret = Promise.resolve(arg0);
        return ret;
    };
    imports.wbg.__wbg_send_ea59e150ab5ebe08 = function() { return handleError(function (arg0, arg1, arg2) {
        arg0.send(getArrayU8FromWasm0(arg1, arg2));
    }, arguments) };
    imports.wbg.__wbg_setInterval_e554642fb765ad65 = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = arg0.setInterval(arg1, arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_setTimeout_06477c23d31efef1 = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = arg0.setTimeout(arg1, arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_set_3f1d0b984ed272ed = function(arg0, arg1, arg2) {
        arg0[arg1] = arg2;
    };
    imports.wbg.__wbg_set_781438a03c0c3c81 = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = Reflect.set(arg0, arg1, arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_set_7df433eea03a5c14 = function(arg0, arg1, arg2) {
        arg0[arg1 >>> 0] = arg2;
    };
    imports.wbg.__wbg_set_binaryType_73e8c75df97825f8 = function(arg0, arg1) {
        arg0.binaryType = __wbindgen_enum_BinaryType[arg1];
    };
    imports.wbg.__wbg_set_efaaf145b9377369 = function(arg0, arg1, arg2) {
        const ret = arg0.set(arg1, arg2);
        return ret;
    };
    imports.wbg.__wbg_set_method_76c69e41b3570627 = function(arg0, arg1, arg2) {
        arg0.method = getStringFromWasm0(arg1, arg2);
    };
    imports.wbg.__wbg_set_mode_611016a6818fc690 = function(arg0, arg1) {
        arg0.mode = __wbindgen_enum_RequestMode[arg1];
    };
    imports.wbg.__wbg_set_onclose_032729b3d7ed7a9e = function(arg0, arg1) {
        arg0.onclose = arg1;
    };
    imports.wbg.__wbg_set_onerror_7819daa6af176ddb = function(arg0, arg1) {
        arg0.onerror = arg1;
    };
    imports.wbg.__wbg_set_onmessage_71321d0bed69856c = function(arg0, arg1) {
        arg0.onmessage = arg1;
    };
    imports.wbg.__wbg_set_onopen_6d4abedb27ba5656 = function(arg0, arg1) {
        arg0.onopen = arg1;
    };
    imports.wbg.__wbg_sign_e79016c7732f63f9 = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
        const ret = arg0.sign(getStringFromWasm0(arg1, arg2), arg3, arg4);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_stack_0ed75d68575b0f3c = function(arg0, arg1) {
        const ret = arg1.stack;
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg_static_accessor_GLOBAL_769e6b65d6557335 = function() {
        const ret = typeof global === 'undefined' ? null : global;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_static_accessor_GLOBAL_THIS_60cf02db4de8e1c1 = function() {
        const ret = typeof globalThis === 'undefined' ? null : globalThis;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_static_accessor_SELF_08f5a74c69739274 = function() {
        const ret = typeof self === 'undefined' ? null : self;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_static_accessor_WINDOW_a8924b26aa92d024 = function() {
        const ret = typeof window === 'undefined' ? null : window;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_status_9bfc680efca4bdfd = function(arg0) {
        const ret = arg0.status;
        return ret;
    };
    imports.wbg.__wbg_subarray_845f2f5bce7d061a = function(arg0, arg1, arg2) {
        const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
        return ret;
    };
    imports.wbg.__wbg_subtle_0109c00de0ea1788 = function(arg0) {
        const ret = arg0.subtle;
        return ret;
    };
    imports.wbg.__wbg_then_429f7caf1026411d = function(arg0, arg1, arg2) {
        const ret = arg0.then(arg1, arg2);
        return ret;
    };
    imports.wbg.__wbg_then_4f95312d68691235 = function(arg0, arg1) {
        const ret = arg0.then(arg1);
        return ret;
    };
    imports.wbg.__wbg_torclient_new = function(arg0) {
        const ret = TorClient.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_verify_14e9795160902675 = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
        const ret = arg0.verify(arg1, arg2, arg3, arg4);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_versions_c01dfd4722a88165 = function(arg0) {
        const ret = arg0.versions;
        return ret;
    };
    imports.wbg.__wbg_warn_1529a2c662795cd8 = function(arg0, arg1) {
        var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
        wasm.__wbindgen_free(arg0, arg1 * 4, 4);
        console.warn(...v0);
    };
    imports.wbg.__wbg_warn_6e567d0d926ff881 = function(arg0) {
        console.warn(arg0);
    };
    imports.wbg.__wbg_wasClean_4154a2d59fdb4dd7 = function(arg0) {
        const ret = arg0.wasClean;
        return ret;
    };
    imports.wbg.__wbindgen_cast_2241b6af4c4b2941 = function(arg0, arg1) {
        // Cast intrinsic for `Ref(String) -> Externref`.
        const ret = getStringFromWasm0(arg0, arg1);
        return ret;
    };
    imports.wbg.__wbindgen_cast_4625c577ab2ec9ee = function(arg0) {
        // Cast intrinsic for `U64 -> Externref`.
        const ret = BigInt.asUintN(64, arg0);
        return ret;
    };
    imports.wbg.__wbindgen_cast_5d638ce117b395b6 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 1535, function: Function { arguments: [NamedExternref("CloseEvent")], shim_idx: 1536, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen__closure__destroy__h00070d11570d9b01, wasm_bindgen__convert__closures_____invoke__h437f43f99c40f53c);
        return ret;
    };
    imports.wbg.__wbindgen_cast_88fdcfb6b716b7f8 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 2473, function: Function { arguments: [Externref], shim_idx: 2474, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen__closure__destroy__hf76ded83d5f84246, wasm_bindgen__convert__closures_____invoke__h32d5e12558544916);
        return ret;
    };
    imports.wbg.__wbindgen_cast_8a243ce043302cf9 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 1535, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 1536, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen__closure__destroy__h00070d11570d9b01, wasm_bindgen__convert__closures_____invoke__h437f43f99c40f53c);
        return ret;
    };
    imports.wbg.__wbindgen_cast_9ae0607507abb057 = function(arg0) {
        // Cast intrinsic for `I64 -> Externref`.
        const ret = arg0;
        return ret;
    };
    imports.wbg.__wbindgen_cast_9e624796bcb7fdb9 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 1572, function: Function { arguments: [], shim_idx: 1573, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen__closure__destroy__h03f628fb5c1af182, wasm_bindgen__convert__closures_____invoke__h15fcc68dda9f98b6);
        return ret;
    };
    imports.wbg.__wbindgen_cast_cb9088102bce6b30 = function(arg0, arg1) {
        // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
        const ret = getArrayU8FromWasm0(arg0, arg1);
        return ret;
    };
    imports.wbg.__wbindgen_cast_cbf7ce177ed477f1 = function(arg0, arg1) {
        var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
        wasm.__wbindgen_free(arg0, arg1 * 4, 4);
        // Cast intrinsic for `Vector(NamedExternref("JsCircuitRelay")) -> Externref`.
        const ret = v0;
        return ret;
    };
    imports.wbg.__wbindgen_cast_cdc6e87a965f9a20 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 1535, function: Function { arguments: [NamedExternref("ErrorEvent")], shim_idx: 1536, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, wasm.wasm_bindgen__closure__destroy__h00070d11570d9b01, wasm_bindgen__convert__closures_____invoke__h437f43f99c40f53c);
        return ret;
    };
    imports.wbg.__wbindgen_cast_d6cd19b81560fd6e = function(arg0) {
        // Cast intrinsic for `F64 -> Externref`.
        const ret = arg0;
        return ret;
    };
    imports.wbg.__wbindgen_init_externref_table = function() {
        const table = wasm.__wbindgen_externrefs;
        const offset = table.grow(4);
        table.set(0, undefined);
        table.set(offset + 0, undefined);
        table.set(offset + 1, null);
        table.set(offset + 2, true);
        table.set(offset + 3, false);
    };

    return imports;
}

function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    __wbg_init.__wbindgen_wasm_module = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;


    wasm.__wbindgen_start();
    return wasm;
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (typeof module !== 'undefined') {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (typeof module_or_path !== 'undefined') {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (typeof module_or_path === 'undefined') {
        module_or_path = new URL('webtor_demo_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync };
export default __wbg_init;
