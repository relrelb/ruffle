/* eslint @typescript-eslint/no-explicit-any: "off" */

/**
 * Conditional ruffle loader
 */

import init, { Ruffle } from "../pkg/ruffle_web";
import { setPolyfillsOnLoad } from "./js-polyfills";

/**
 * Load ruffle from an automatically-detected location.
 *
 * This function returns a new instance of Ruffle and downloads it every time.
 * You should not use it directly; this module will memoize the resource
 * download.
 *
 * @returns A ruffle constructor that may be used to create new Ruffle
 * instances.
 */
async function fetchRuffle(): Promise<{ new (...args: any[]): Ruffle }> {
    // Apply some pure JavaScript polyfills to prevent conflicts with external
    // libraries, if needed.
    setPolyfillsOnLoad();

    // wasm files are set to use file-loader,
    // so this package will resolve to the URL of the wasm file.
    const ruffleWasm = await import(
        /* webpackMode: "eager" */
        "../pkg/ruffle_web_bg.wasm"
    );
    await init(ruffleWasm.default);

    return Ruffle;
}

let lastLoaded: Promise<{ new (...args: any[]): Ruffle }> | null = null;

/**
 * Obtain an instance of `Ruffle`.
 *
 * This function returns a promise which yields `Ruffle` asynchronously.
 *
 * @returns A ruffle constructor that may be used to create new Ruffle
 * instances.
 */
export function loadRuffle(): Promise<{ new (...args: any[]): Ruffle }> {
    if (lastLoaded == null) {
        lastLoaded = fetchRuffle();
    }

    return lastLoaded;
}
