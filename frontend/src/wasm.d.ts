// Type declarations for dynamically imported WASM module
// The actual module is generated at build time by wasm-pack

declare module '*/wasm/sol_beast_wasm' {
  export default function init(): Promise<void>
  export function init(): Promise<void>
  export class SolBeastBot {
    constructor()
    start(): void
    stop(): void
    is_running(): boolean
    get_mode(): string
    set_mode(mode: string): void
    get_settings(): string
    update_settings(settings: string): void
    get_logs(): string
    get_holdings(): string
    get_detected_tokens(): string
    build_buy_transaction(mint: string, userPubkey: string): string
    // Phase 4: Holdings Management
    add_holding(mint: string, amount: bigint, buy_price: number, metadata_json: string | null): void
    monitor_holdings(): Promise<string>
    build_sell_transaction(mint: string, userPubkey: string): string
    remove_holding(mint: string, profit_percent: number, reason: string): void
    test_rpc_connection(): Promise<string>
    test_ws_connection(): Promise<string>
    save_to_storage(): void
    load_from_storage(): void
  }
}
