# sol_beast_scripts

This folder contains wrapper scripts to run the project in a cross-platform manner. It is a staging location to keep repository root tidy while maintaining compatibility with older scripts.

Usage examples:

Linux/macOS:

```bash
# Start backend
./sol_beast_scripts/linux/start-backend.sh

# Start frontend
./sol_beast_scripts/linux/start-frontend.sh

# Deploy/build
./sol_beast_scripts/linux/deploy.sh check/setup/start-backend/start-frontend
```

Windows:

```
# Start backend in Command Prompt
sol_beast_scripts\windows\start-backend.bat

# Start frontend
sol_beast_scripts\windows\start-frontend.bat

# Deploy
sol_beast_scripts\windows\deploy.bat
```

Note: These are wrapper scripts that currently delegate to the original scripts in the repository root to keep compatibility.