# --- Étape 1 : Construction ---
# On utilise l'image Rust officielle
FROM rust:latest AS builder

# 1. On installe la cible "MUSL"
# C'est ce qui nous permet de créer un binaire statique pour Alpine
RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /usr/src/app
COPY . .

# 2. On compile le bot pour la cible MUSL
# (Le nom de votre projet 'sol_beast' est utilisé ici)
RUN cargo build --release --target x86_64-unknown-linux-musl

# --- Étape 2 : L'image finale ---
# On utilise Alpine, l'une des images les plus légères
FROM alpine:latest

# On crée un répertoire de travail
WORKDIR /app

# 3. On copie votre binaire 'sol_beast'
# (Notez le chemin différent à cause de la cible MUSL)
COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-musl/release/sol_beast /app/bot

# 4. (Important) On installe les certificats
# Si votre bot fait des appels HTTPS vers le RPC Solana, vous AVEZ BESOIN de ça.
# Sinon, Alpine est si 'nu' que les connexions SSL échoueront.
RUN apk add --no-cache ca-certificates

# 5. La commande de démarrage (Méthode 2)
# Crée le fichier 'cargo.toml' à partir du secret, puis lance le bot.
CMD ["sh", "-c", "echo \"$CONFIG_FILE_CONTENT\" > /app/cargo.toml && /app/bot"]