// Shim Buffer in the browser early to prevent runtime errors
import { Buffer } from 'buffer'

// Assign Buffer to window/globalThis so older libraries can access it
;(window as any).Buffer = Buffer

// Ensure global is available for legacy libs
;(window as any).global = (window as any).global || window
;(window as any).globalThis = (window as any).globalThis || window

export default {}
