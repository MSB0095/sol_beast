// CLI entrypoint for sol_beast
// This wraps the existing functionality while preparing for modularization

// Re-export all modules from the root src directory for now
// This maintains backward compatibility while we transition
include!("../../src/main.rs");
