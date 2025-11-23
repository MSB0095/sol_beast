// CLI entrypoint for sol_beast
// This wraps the existing functionality while preparing for modularization

// TODO: Refactor to proper module structure instead of include!
// For now, this maintains backward compatibility while we transition
// the codebase to the new architecture. Future work should move
// the implementation into proper modules within sol_beast_cli.
include!("../../src/main.rs");
