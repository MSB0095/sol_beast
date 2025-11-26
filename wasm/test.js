const { SolBeastBot } = require('./pkg-node/sol_beast_wasm.js');

// Mock window and other browser APIs
global.window = {
  setInterval: (fn, ms) => {
    console.log(`Mock setInterval called with ${ms}ms`);
    return setInterval(fn, ms);
  },
  clearInterval: (id) => {
    console.log(`Mock clearInterval called`);
    clearInterval(id);
  },
};

global.self = global;

console.log('Testing SolBeastBot WASM...');

try {
  const bot = new SolBeastBot();
  console.log('Bot created successfully');

  // Initialize with pump program
  const pumpProgram = '6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P';
  bot.initialize(pumpProgram);
  console.log('Bot initialized');

  // Test calculate_token_output
  const tokens = bot.calculate_token_output(0.01, 1000.0, 1000000.0);
  console.log('Calculated tokens:', tokens);

  // Get logs
  const logs = bot.get_logs();
  console.log('Initial logs:', logs);

  // Get stats
  const stats = bot.get_stats();
  console.log('Initial stats:', stats);

  // Test start_bot (commented out to avoid WS connection in test)
  // try {
  //   bot.start_bot('https://api.mainnet-beta.solana.com', 'wss://api.mainnet-beta.solana.com');
  //   console.log('Bot started successfully');
  // } catch (e) {
  //   console.log('Bot start failed (expected in test environment):', e.message);
  // }

  console.log('Test completed successfully');

} catch (error) {
  console.error('Error:', error);
  process.exit(1);
}