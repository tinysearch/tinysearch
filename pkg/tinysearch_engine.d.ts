/* tslint:disable */
/**
* @param {string} query 
* @param {number} num_results 
* @returns {any} 
*/
export function search(query: string, num_results: number): any;

/**
* If `module_or_path` is {RequestInfo}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {RequestInfo | BufferSource | WebAssembly.Module} module_or_path
*
* @returns {Promise<any>}
*/
export function init (module_or_path: RequestInfo | BufferSource | WebAssembly.Module): Promise<any>;
        