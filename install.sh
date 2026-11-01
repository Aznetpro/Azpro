#!/bin/bash
# Mpro Installer - Version v3

REPO_URL="https://github.com/Jbpro2/wzpro.git"
REPO_BRANCH="main"
CMD_NAME="wzpro"
TOTAL_STEPS=7

CURRENT_STEP=0

# --- Cores e Estilos ---
GREEN="\033[0;32m"
BLUE="\033[0;34m"
RED="\033[0;31m"
NC="\033[0m"
BOLD="\033[1m"

log_info() {
    echo -e "${BLUE}${BOLD}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}${BOLD}[SUCESSO]${NC} $1"
}

log_error() {
    echo -e "${RED}${BOLD}[ERRO]${NC} $1"
    exit 1
}

show_progress() {
    CURRENT_STEP=$((CURRENT_STEP + 1))
    PERCENT=$((CURRENT_STEP * 100 / TOTAL_STEPS))
    log_info "${PERCENT}% - $1"
}

# --- Verificação de Root ---
if [ "$EUID" -ne 0 ]; then
    log_error "Este script precisa ser executado como ROOT."
fi

clear
echo -e "${CYAN}${BOLD}    W S P R O  F R E E  V E R S I O N${NC}"
echo -e "${BLUE}${BOLD}----------------------------------------${NC}"
log_info "Iniciando instalação do WSPRO v3.1.0 (🇧🇷)..."

# --- Etapa 1 ---
show_progress "Atualizando dependências..."
apt update -y > /dev/null 2>&1
apt install -y curl build-essential git libssl-dev pkg-config openssl > /dev/null 2>&1

# --- Etapa 2 ---
show_progress "Verificando Rust..."
if ! command -v cargo &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y > /dev/null 2>&1
    source "$HOME/.cargo/env"
else
    source "$HOME/.cargo/env"
fi

# --- Etapa 3 ---
show_progress "Baixando código fonte..."
rm -rf /tmp/Mpro_build
git clone --branch "$REPO_BRANCH" "$REPO_URL" /tmp/Mpro_build > /dev/null 2>&1 || log_error "Falha ao clonar repositório."
cd /tmp/Mpro_build || log_error "Falha ao acessar diretório."

# --- Etapa 4 ---
show_progress "Compilando (2-5 min)..."
cargo build --release > /tmp/mpro_build.log 2>&1
if [ $? -ne 0 ]; then
    cat /tmp/mpro_build.log
    log_error "Falha na compilação. Veja logs acima."
fi

# --- Etapa 5 ---
show_progress "Instalando binários..."
mkdir -p /opt/mpro

# Parar somente os binários do proxy; não encerrar o menu wzpro que iniciou
# esta atualização.
systemctl stop proxy-websocket.service > /dev/null 2>&1 || true
systemctl disable proxy-websocket.service > /dev/null 2>&1 || true
pkill -f '^/opt/mpro/proxy-websocket([[:space:]]|$)' > /dev/null 2>&1 || true
rm -f /etc/systemd/system/proxy-websocket.service /opt/mpro/proxy-websocket
systemctl daemon-reload > /dev/null 2>&1 || true
pkill -f '^/opt/mpro/proxy([[:space:]]|$)' > /dev/null 2>&1 || true
pkill -f '^/opt/mpro/proxy-xhttp([[:space:]]|$)' > /dev/null 2>&1 || true
pkill -f '^/opt/mpro/proxy-integrated([[:space:]]|$)' > /dev/null 2>&1 || true
sleep 1

# Copiar com força (-f)
cp -f ./target/release/mpro /opt/mpro/proxy || log_error "Falha ao copiar mpro. Verifique se o disco está cheio."
chmod +x /opt/mpro/proxy

if [ -f ./target/release/mpro-xhttp ]; then
    cp -f ./target/release/mpro-xhttp /opt/mpro/proxy-xhttp
    chmod +x /opt/mpro/proxy-xhttp
    ln -sf /opt/mpro/proxy-xhttp /usr/local/bin/mpro-xhttp
fi

if [ -f ./target/release/mpro-integrated ]; then
    cp -f ./target/release/mpro-integrated /opt/mpro/proxy-integrated
    chmod +x /opt/mpro/proxy-integrated
    ln -sf /opt/mpro/proxy-integrated /usr/local/bin/mpro-integrated
fi

# Menu
if [ -f "menu.sh" ]; then
    cp -f menu.sh /opt/mpro/menu
    chmod +x /opt/mpro/menu
    ln -sf /opt/mpro/menu "/usr/local/bin/${CMD_NAME}"
    ln -sf /opt/mpro/menu /usr/local/bin/wspro
    ln -sf /opt/mpro/menu /usr/local/bin/mpro
fi

# Certificados
if [ ! -f /opt/mpro/cert.pem ]; then
    openssl req -x509 -newkey rsa:2048 -keyout /opt/mpro/key.pem -out /opt/mpro/cert.pem -days 3650 -nodes -subj "/CN=Mpro" 2>/dev/null
fi

# --- Etapa 6 ---
show_progress "Limpando..."
rm -rf /tmp/Mpro_build

# --- Etapa 7 ---
log_success "Instalação concluída com sucesso!"
echo -e "Use o comando ${YELLOW}${CMD_NAME}${NC} para abrir o menu."
echo -e "--------------------------------------------------------------"
