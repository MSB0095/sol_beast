#!/bin/bash

# Sol Beast Remote Access - Quick Reference
# Run this script to see all available tunnel options

cat << 'EOF'
╔══════════════════════════════════════════════════════════════════╗
║            Sol Beast Remote Access - Quick Reference            ║
╚══════════════════════════════════════════════════════════════════╝

🚀 OPTION 1: ngrok Tunnel (RECOMMENDED - NO PASSWORD!)
   Best option - visitors never see password screen!
   
   Terminal 1:  ./start.sh cli
   Terminal 2:  ./scripts/ngrok-tunnel.sh
   
   Share: https://random-id.ngrok.io (from ngrok output)
   ✅ NO password screen ever!
   ✅ HTTPS included automatically!
   ✅ Most reliable option!
   ⚠️  Requires free ngrok account (one-time setup)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🌐 OPTION 2: localtunnel (with password - free, no signup)
   Visitors need to enter password once
   Uses random URL each time
   
   ./start.sh cli --tunnel
   
   Share: https://random-id.loca.lt + your public IP as password
   ⚠️  Visitors must enter your public IP as password (one-time per 7 days)
   ✅ No signup required
   ✅ Each run gets unique URL - no conflicts!

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🔧 OPTION 3: Manual Tunnel + Proxy
   Full control over each component
   
   Terminal 1:  ./start.sh cli
   Terminal 2:  ./scripts/tunnel.sh
   Terminal 3:  node scripts/tunnel-proxy.js TUNNEL_URL 8888
   
   Share: http://YOUR_PUBLIC_IP:8888

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

💡 TIPS:

Get your public IP:
   curl ifconfig.me

Check if proxy is running:
   curl http://localhost:8888

Open firewall port (Linux):
   sudo ufw allow 8888/tcp

Check logs:
   Tunnel:  /tmp/sol_beast_tunnel.log
   Proxy:   /tmp/sol_beast_proxy.log

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🔒 SECURITY WARNING:
   ⚠️  No authentication enabled on the API!
   ⚠️  Anyone with the URL can control your trading bot!
   ⚠️  Only share with trusted users!
   ⚠️  Stop the tunnel when not needed!

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📚 MORE INFO:
   Full guide:        cat TUNNEL_BYPASS.md
   Alternatives:      cat TUNNEL_ALTERNATIVES.md
   Troubleshooting:   grep -A 5 "Troubleshooting" TUNNEL_BYPASS.md

╚══════════════════════════════════════════════════════════════════╝
EOF
