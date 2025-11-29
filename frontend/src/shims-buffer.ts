// Shim Buffer in the browser early to prevent runtime errors
import { Buffer } from 'buffer'

// Provide types for window augmentations we need in the browser
declare global {
	interface Window {
		Buffer?: typeof Buffer
		global?: Window
		globalThis?: Window
		// allow additional dynamic properties
		[key: string]: unknown
	}
}

// Assign Buffer to window/globalThis so older libraries can access it
window.Buffer = Buffer

// Ensure global is available for legacy libs
window.global = window.global || window
window.globalThis = window.globalThis || window

export default {}
