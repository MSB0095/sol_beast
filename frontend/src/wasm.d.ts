// Type declarations for dynamically imported WASM module
// The actual module is generated at build time by wasm-pack

declare module '*/wasm/sol_beast_wasm' {
  export default function init(): Promise<void>
  export function init(): Promise<void>
  export class SolBeastBot {
    constructor()
    start(): Promise<any>
    stop(): Promise<any>
    get_state(): Promise<any>
    get_stats(): Promise<any>
    get_logs(): Promise<any>
    get_detected_coins(): Promise<any>
    set_mode(mode: string): Promise<any>
  }
}
