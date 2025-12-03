# Common Issues

## Bot Won't Start

**Symptoms:** Bot fails to initialize or immediately stops

**Solutions:**
- Verify RPC endpoint is accessible
- Check WebSocket URL is correct
- Ensure wallet is connected (WASM mode)
- Review logs for specific error messages

## No Tokens Detected

**Symptoms:** Bot runs but never detects tokens

**Solutions:**
- Verify pump.fun program ID is correct
- Check WebSocket connection status
- Ensure RPC supports account subscriptions
- Look for WebSocket errors in logs

## Transactions Failing

**Symptoms:** Buys/sells fail to execute

**Solutions:**
- Check SOL balance is sufficient
- Verify slippage tolerance
- Ensure wallet approval (WASM mode)
- Review priority fee settings
- Check Helius Sender configuration

See [FAQ](./faq.md) for more common questions.
