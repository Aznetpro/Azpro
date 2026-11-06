#!/bin/bash

# ============================================
# Azpro Menu - Free v5
# ============================================

LKPROXY="/opt/mpro/proxy"
LKPROXY_XHTTP="/opt/mpro/proxy-xhttp"
LKPROXY_INTEGRATED="/opt/mpro/proxy-integrated"
BYPASS_DIR="/opt/mpro"
LEGACY_MULTIPROTOCOL_FLAG="${BYPASS_DIR}/multiprotocol.enabled"
SYSTEMD_DIR="/etc/systemd/system"

# Cores
RED='\033[0;31m'
GREEN='\033[0;32m'
LIGHT_GREEN='\033[1;92m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
WHITE='\033[0;37m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m'

BOX_WIDTH=46

strip_len() {
    local clean
    clean=$(echo -ne "$1" | sed -r 's/\x1B\[[0-9;]*[mK]//g')
    echo -n "${#clean}"
}

box_line() {
    local content="$1"
    local visible_len
    visible_len=$(strip_len "$content")
    local pad=$(( BOX_WIDTH - visible_len ))
    [ $pad -lt 0 ] && pad=0
    echo -ne "${CYAN}║${NC} "
    echo -ne "${content}"
    printf '%*s' "$pad" ''
    echo -e " ${CYAN}║${NC}"
}

box_top() {
    printf "${CYAN}╔"
    printf '═%.0s' $(seq 1 $((BOX_WIDTH + 2)))
    printf "╗${NC}\n"
}

box_mid() {
    printf "${CYAN}╠"
    printf '═%.0s' $(seq 1 $((BOX_WIDTH + 2)))
    printf "╣${NC}\n"
}

box_bottom() {
    printf "${CYAN}╚"
    printf '═%.0s' $(seq 1 $((BOX_WIDTH + 2)))
    printf "╝${NC}\n"
}

show_banner() {
echo -e "${CYAN}${BOLD}    A Z P R O  P R O T O C O L O  F R E E${NC}"
echo -e "${BLUE}${BOLD}-------------------------------------------${NC}"
}

has_any_bypass_enabled() {
    local FLAG
    for FLAG in "${BYPASS_DIR}"/bypass-http-ssh-*.enabled; do
        [[ -f "$FLAG" ]] && return 0
    done
    return 1
}

has_active_azpro_service() {
    local SERVICE_FILE SERVICE
    for SERVICE_FILE in "${SYSTEMD_DIR}"/proxy-*.service; do
        [[ -f "$SERVICE_FILE" ]] || continue
        grep -Eq '^[[:space:]]*ExecStart=.*/opt/mpro/(proxy|proxy-xhttp|proxy-integrated|proxy-websocket)([[:space:]]|$)' "$SERVICE_FILE" || continue
        SERVICE=$(basename "$SERVICE_FILE")
        systemctl is-active --quiet "$SERVICE" 2>/dev/null && return 0
    done
    return 1
}

show_execution_status() {
    if has_active_azpro_service; then
        box_line "Status: EM EXECUÇÃO (${LIGHT_GREEN}ONLINE${NC})"
    else
        box_line "Status: EM EXECUÇÃO (${RED}OFFLINE${NC})"
    fi
}

show_active_ports() {
    ACTIVE=""
    XHTTP_ACTIVE=false
    INTEGRATED_ACTIVE=false

    for service_file in ${SYSTEMD_DIR}/proxy-*.service; do
        if [ -f "$service_file" ]; then
            NAME=$(basename "$service_file" .service | sed 's/proxy-//')
            if systemctl is-active --quiet "proxy-${NAME}.service" 2>/dev/null; then
                if [ "$NAME" = "443" ]; then
                    XHTTP_ACTIVE=true
                elif [ "$NAME" = "integrated" ]; then
                    INTEGRATED_ACTIVE=true
                else
                    ACTIVE="$ACTIVE $NAME"
                fi
            fi
        fi
    done

    local ports_str=""
    if [ -n "$ACTIVE" ]; then
        ports_str="${YELLOW}${ACTIVE# }${NC}"
    fi
    if [ "$XHTTP_ACTIVE" = true ]; then
        ports_str="${ports_str}${ports_str:+ }${YELLOW}443${NC}"
    fi
    if [ "$INTEGRATED_ACTIVE" = true ]; then
        ports_str="${ports_str}${ports_str:+ }${GREEN}INTEGRADO${NC}"
    fi
    if [ -z "$ports_str" ]; then
        ports_str="${RED}nenhuma${NC}"
    fi

    box_line "${YELLOW}Porta(s) ativa(s):${NC} ${ports_str}"
    if has_any_bypass_enabled; then
        box_line "${CYAN}BYPASS PROTOCOLO:${NC} ${LIGHT_GREEN}ATIVO${NC}"
    else
        box_line "${CYAN}BYPASS PROTOCOLO:${NC} ${RED}OFFLINE${NC}"
    fi
}

create_service() {
    local PORT=$1
    local STATUS=$2
    local SSH_ONLY=$3
    local HTTPS=$4
    local SERVICE_FILE="${SYSTEMD_DIR}/proxy-${PORT}.service"
    EXTRA_ARGS="-p ${PORT}"
    [[ -n "$STATUS" ]] && EXTRA_ARGS="${EXTRA_ARGS} -s ${STATUS}"
    [[ "$SSH_ONLY" == "s" ]] && EXTRA_ARGS="${EXTRA_ARGS} -ssh"
    if [[ "$HTTPS" == "s" ]]; then
        EXTRA_ARGS="${EXTRA_ARGS} -t"
    else
        EXTRA_ARGS="${EXTRA_ARGS} --no-tls"
    fi
    cat > "$SERVICE_FILE" << EOF
[Unit]
Description=Mpro - Porta ${PORT}
After=network.target
[Service]
Type=simple
ExecStart=${LKPROXY} ${EXTRA_ARGS}
Restart=on-failure
RestartSec=5
User=root
WorkingDirectory=/opt/mpro
[Install]
WantedBy=multi-user.target
EOF
}

create_xhttp_service() {
    local PORT=$1
    local STATUS=$2
    local SSH_PORT=$3
    local SERVICE_FILE="${SYSTEMD_DIR}/proxy-${PORT}.service"
    EXTRA_ARGS="-p ${PORT} -s ${STATUS} --ssh-port ${SSH_PORT}"
    cat > "$SERVICE_FILE" << EOF
[Unit]
Description=Mpro xHTTP + SSL Tunnel - Porta ${PORT}
After=network.target
[Service]
Type=simple
ExecStart=${LKPROXY_XHTTP} ${EXTRA_ARGS}
Restart=on-failure
RestartSec=5
User=root
WorkingDirectory=/opt/mpro
[Install]
WantedBy=multi-user.target
EOF
}

open_port() {
    clear
    show_banner
    echo ""
    box_top
    box_line "${WHITE}${BOLD}Abrir Porta${NC}"
    box_mid
    box_line "${WHITE}Portas padrão: 80, 8080, 8880, 3128${NC}"
    box_line "${YELLOW}Porta 443: use opção [04] PROXY + PROTOCOLO${NC}"
    box_bottom
    echo ""
    read -p "Porta: " PORT
    [[ -z "$PORT" ]] && return
    if systemctl is-active --quiet "proxy-${PORT}.service" 2>/dev/null; then
        echo -e "${RED}Porta ${PORT} já está em uso!${NC}"; sleep 2; return
    fi
    read -r -p "Habilitar o HTTPS/TLS? (s/n): " HTTPS
    HTTPS=${HTTPS,,}
    [[ -z "$HTTPS" ]] && HTTPS="n"
    if [[ "$HTTPS" != "s" && "$HTTPS" != "n" ]]; then
        echo -e "${RED}Resposta inválida. Use s ou n.${NC}"
        read -r -p "Enter pra continuar..."
        return
    fi
    read -r -p "Status HTTP (Padrão: @Azpro): " STATUS
    [[ -z "$STATUS" ]] && STATUS="@Azpro"
    read -p "Habilitar somente SSH? (s/n): " SSH_ONLY
    SSH_ONLY=${SSH_ONLY,,}
    mkdir -p /opt/mpro
    create_service "$PORT" "$STATUS" "$SSH_ONLY" "$HTTPS"
    systemctl daemon-reload
    systemctl enable "proxy-${PORT}.service" 2>/dev/null
    systemctl start "proxy-${PORT}.service" 2>/dev/null
    sleep 2
    read -p "Enter pra continuar..."
}

open_xhttp() {
    clear
    show_banner
    echo ""
    box_top
    box_line "${WHITE}${BOLD}xHTTP_SSH / SSL TUNNEL - Porta 443${NC}"
    box_bottom
    echo ""
    read -r -p "Nome do status HTTP: " STATUS
    [[ -z "$STATUS" ]] && STATUS="@Azpro"
    read -p "Porta SSH backend (Padrão: 22): " SSH_PORT
    [[ -z "$SSH_PORT" ]] && SSH_PORT="22"
    mkdir -p /opt/mpro
    # Um serviço já ativo não recebe os novos argumentos com `systemctl start`.
    # Pare-o antes de sobrescrever a unit para garantir que o status novo seja aplicado.
    if systemctl is-active --quiet "proxy-443.service" 2>/dev/null; then
        systemctl stop "proxy-443.service" || {
            echo -e "${RED}Não foi possível parar proxy-443.service; configuração não alterada.${NC}"
            read -r -p "Enter para continuar..." _
            return 1
        }
    fi
    if [ ! -f "/opt/mpro/cert.pem" ]; then
        openssl req -x509 -newkey rsa:2048 -keyout /opt/mpro/key.pem -out /opt/mpro/cert.pem -days 365 -nodes -subj "/CN=mpro" 2>/dev/null
    fi
    create_xhttp_service "443" "$STATUS" "$SSH_PORT"
    systemctl daemon-reload
    systemctl enable "proxy-443.service" 2>/dev/null
    if ! systemctl start "proxy-443.service" 2>/dev/null; then
        echo -e "${RED}Não foi possível iniciar proxy-443.service com o novo status.${NC}"
        read -r -p "Enter para continuar..." _
        return 1
    fi
    echo -e "${GREEN}Status HTTP aplicado: 200 ${STATUS}${NC}"
    sleep 2
    read -p "Enter pra continuar..."
}

open_integrated() {
    clear
    echo -e "${CYAN}+--------------------------------------------------------+${NC}"
    echo -e "${CYAN}+--------------------------------------------------------+${NC}"
    echo -e "${WHITE}|  CONFIGURACOES ATUAIS                                  |${NC}"
    echo -e "${CYAN}+--------------------------------------------------------+${NC}"
    echo -e "${WHITE}|  Porta: 8000                                           |${NC}"
    echo -e "${WHITE}|  Sub-rede: 10.10.0.0/16                                |${NC}"
    echo -e "${WHITE}|  Interface TUN: tun0                                   |${NC}"
    echo -e "${WHITE}|  Protocolos: tcp:8000,udp:8000,quic:8001               |${NC}"
    echo -e "${CYAN}+--------------------------------------------------------+${NC}"
    read -p "Porta (Enter para manter [8000]): " PORT
    [[ -z "$PORT" ]] && PORT="8000"
    read -p "Sub-rede CIDR (Enter para manter [10.10.0.0/16]): " SUBNET
    [[ -z "$SUBNET" ]] && SUBNET="10.10.0.0/16"
    read -p "Interface TUN (Enter para manter [tun0]): " TUN
    [[ -z "$TUN" ]] && TUN="tun0"
    read -p "Deseja ativar UDP na mesma porta? (s/N) " ENABLE_UDP
    UDP_ARG=""
    [[ "$ENABLE_UDP" == "s" || "$ENABLE_UDP" == "S" ]] && UDP_ARG="--udp"
    read -p "Deseja ativar QUIC? (s/N) " ENABLE_QUIC
    QUIC_ARG=""
    if [[ "$ENABLE_QUIC" == "s" || "$ENABLE_QUIC" == "S" ]]; then
        read -p "Porta para QUIC (Enter para 8001): " QPORT
        [[ -z "$QPORT" ]] && QPORT="8001"
        QUIC_ARG="--quic --quic-port $QPORT"
    fi
    read -r -p "Nome do status HTTP: " STATUS
    [[ -z "$STATUS" ]] && STATUS="@Azpro"
    mkdir -p /opt/mpro
    if [ ! -f "/opt/mpro/cert.pem" ]; then
        openssl req -x509 -newkey rsa:2048 -keyout /opt/mpro/key.pem -out /opt/mpro/cert.pem -days 365 -nodes -subj "/CN=mpro" 2>/dev/null
    fi
    cat > "${SYSTEMD_DIR}/proxy-integrated.service" << EOF
[Unit]
Description=Mpro Integrated
After=network.target
[Service]
Type=simple
ExecStart=${LKPROXY_INTEGRATED} -p ${PORT} -s ${STATUS} --tun ${TUN} --subnet ${SUBNET} ${UDP_ARG} ${QUIC_ARG}
Restart=on-failure
RestartSec=5
User=root
WorkingDirectory=/opt/mpro
[Install]
WantedBy=multi-user.target
EOF
    systemctl daemon-reload
    systemctl enable proxy-integrated.service 2>/dev/null
    systemctl start proxy-integrated.service 2>/dev/null
    sleep 2
    echo "Servidor protocolo iniciado com sucesso!"
    read -p "Pressione Enter para continuar..."
}

bypass_flag_for_port() {
    printf '%s/bypass-http-ssh-%s.enabled' "$BYPASS_DIR" "$1"
}

is_managed_numeric_port_service() {
    local SERVICE_FILE="$1"
    local SERVICE_NAME PORT
    SERVICE_NAME=$(basename "$SERVICE_FILE")
    PORT=${SERVICE_NAME#proxy-}
    PORT=${PORT%.service}
    [[ "$PORT" =~ ^[0-9]+$ ]] || return 1
    grep -Fq "ExecStart=${LKPROXY} " "$SERVICE_FILE"
}

toggle_port_bypass() {
    local PORT="$1" SERVICE="proxy-${1}.service" FLAG CONFIRM="" PREVIOUS=false
    FLAG=$(bypass_flag_for_port "$PORT")
    systemctl is-active --quiet "$SERVICE" 2>/dev/null || {
        echo -e "${RED}A porta ${PORT} não está mais ativa.${NC}"
        read -r -p "Enter para continuar..." _
        return 1
    }

    read -r -p "A porta ${PORT} será reiniciada e as conexões cairão. Continuar? (s/N): " CONFIRM
    [[ "$CONFIRM" != "s" && "$CONFIRM" != "S" ]] && return 0

    if ! mkdir -p "$BYPASS_DIR"; then
        echo -e "${RED}Não foi possível acessar ${BYPASS_DIR}.${NC}"
        read -r -p "Enter para continuar..." _
        return 1
    fi

    if [[ -f "$FLAG" ]]; then
        PREVIOUS=true
        rm -f -- "$FLAG" || return 1
    else
        touch "$FLAG" || {
            echo -e "${RED}Não foi possível ativar o BYPASS na porta ${PORT}.${NC}"
            read -r -p "Enter para continuar..." _
            return 1
        }
    fi

    if systemctl restart "$SERVICE" 2>/dev/null; then
        if [[ "$PREVIOUS" == true ]]; then
            echo -e "${YELLOW}BYPASS SSH_PROXY/HTTP DESATIVADO na porta ${PORT}.${NC}"
        else
            echo -e "${GREEN}BYPASS SSH_PROXY/HTTP ATIVADO na porta ${PORT}.${NC}"
        fi
    else
        if [[ "$PREVIOUS" == true ]]; then
            touch "$FLAG"
        else
            rm -f -- "$FLAG"
        fi
        systemctl restart "$SERVICE" >/dev/null 2>&1 || true
        echo -e "${RED}Falha ao reiniciar ${SERVICE}; configuração anterior restaurada.${NC}"
    fi
    read -r -p "Enter para continuar..." _
}

manage_bypass() {
    local ACTION SERVICE_FILE SERVICE_NAME PORT FLAG INDEX
    local -a ACTIVE_PORTS=()
    while true; do
        ACTIVE_PORTS=()
        for SERVICE_FILE in "${SYSTEMD_DIR}"/proxy-*.service; do
            [[ -f "$SERVICE_FILE" ]] || continue
            is_managed_numeric_port_service "$SERVICE_FILE" || continue
            SERVICE_NAME=$(basename "$SERVICE_FILE")
            systemctl is-active --quiet "$SERVICE_NAME" 2>/dev/null || continue
            PORT=${SERVICE_NAME#proxy-}
            PORT=${PORT%.service}
            ACTIVE_PORTS+=("$PORT")
        done
        if ((${#ACTIVE_PORTS[@]} > 0)); then
            mapfile -t ACTIVE_PORTS < <(printf '%s\n' "${ACTIVE_PORTS[@]}" | sort -n)
        fi

        clear
        show_banner
        echo ""
        box_top
        box_line "${WHITE}${BOLD}BYPASS SSH_PROXY (HTTP)${NC}"
        box_mid
        if ((${#ACTIVE_PORTS[@]} == 0)); then
            box_line "${YELLOW}Nenhuma porta Azpro ativa.${NC}"
        else
            for ((INDEX=0; INDEX<${#ACTIVE_PORTS[@]}; INDEX++)); do
                PORT=${ACTIVE_PORTS[$INDEX]}
                FLAG=$(bypass_flag_for_port "$PORT")
                printf -v ACTION '%02d' "$((INDEX + 1))"
                if [[ -f "$FLAG" ]]; then
                    box_line "${WHITE}[${ACTION}]${NC} Porta ${PORT} - BYPASS ${GREEN}ATIVO${NC}"
                else
                    box_line "${WHITE}[${ACTION}]${NC} Porta ${PORT} - BYPASS ${YELLOW}DESATIVADO${NC}"
                fi
            done
        fi
        box_mid
        box_line "${WHITE}[00]${NC} - VOLTAR"
        box_bottom
        echo ""
        read -r -p "Selecione a porta: " ACTION
        [[ "$ACTION" == "00" || "$ACTION" == "0" || -z "$ACTION" ]] && return

        if [[ "$ACTION" =~ ^[0-9]+$ ]] && ((10#$ACTION >= 1 && 10#$ACTION <= ${#ACTIVE_PORTS[@]})); then
            PORT=${ACTIVE_PORTS[$((10#$ACTION - 1))]}
            toggle_port_bypass "$PORT"
        fi
    done
}

restart_port() {
    local SERVICE_FILE SERVICE NAME LABEL CHOICE INDEX=0 SELECTED=""
    local -a ACTIVE_SERVICES=() ACTIVE_LABELS=()

    while true; do
        ACTIVE_SERVICES=()
        ACTIVE_LABELS=()
        for SERVICE_FILE in "${SYSTEMD_DIR}"/proxy-*.service; do
            [[ -f "$SERVICE_FILE" ]] || continue
            SERVICE=$(basename "$SERVICE_FILE")
            systemctl is-active --quiet "$SERVICE" 2>/dev/null || continue
            NAME=${SERVICE#proxy-}
            NAME=${NAME%.service}
            case "$NAME" in
                integrated) LABEL="INTEGRADO" ;;
                443) LABEL="XHTTP (443)" ;;
                *) LABEL="PORTA ${NAME}" ;;
            esac
            ACTIVE_SERVICES+=("$SERVICE")
            ACTIVE_LABELS+=("$LABEL")
        done

        clear
        show_banner
        echo ""
        box_top
        box_line "${WHITE}${BOLD}Reiniciar porta aberta${NC}"
        box_mid
        if ((${#ACTIVE_SERVICES[@]} == 0)); then
            box_line "${YELLOW}Nenhum serviço Azpro ativo.${NC}"
        else
            for ((INDEX=0; INDEX<${#ACTIVE_SERVICES[@]}; INDEX++)); do
                printf -v CHOICE '%02d' "$((INDEX + 1))"
                box_line "${WHITE}[${CHOICE}]${NC} - ${ACTIVE_LABELS[$INDEX]}"
            done
        fi
        box_mid
        box_line "${WHITE}[00]${NC} - VOLTAR"
        box_bottom
        echo ""
        read -r -p "Escolha a porta para reiniciar: " CHOICE
        [[ "$CHOICE" == "00" || "$CHOICE" == "0" || -z "$CHOICE" ]] && return

        if [[ "$CHOICE" =~ ^[0-9]+$ ]] && ((10#$CHOICE >= 1 && 10#$CHOICE <= ${#ACTIVE_SERVICES[@]})); then
            SELECTED=${ACTIVE_SERVICES[$((10#$CHOICE - 1))]}
            if systemctl restart "$SELECTED" 2>/dev/null; then
                echo -e "${GREEN}${ACTIVE_LABELS[$((10#$CHOICE - 1))]} reiniciado!${NC}"
            else
                echo -e "${RED}Não foi possível reiniciar ${SELECTED}.${NC}"
            fi
            read -r -p "Enter para continuar..." _
        else
            echo -e "${RED}Opção inválida.${NC}"
            sleep 1
        fi
    done
}

close_service() {
    local SERVICE="$1"
    local SERVICE_FILE="${SYSTEMD_DIR}/${SERVICE}"
    if ! systemctl stop "$SERVICE" 2>/dev/null; then
        echo -e "${RED}Não foi possível parar ${SERVICE}; arquivo mantido.${NC}"
        read -r -p "Enter para continuar..." _
        return 1
    fi
    systemctl disable "$SERVICE" 2>/dev/null || true
    rm -f "$SERVICE_FILE"
    systemctl daemon-reload
    echo -e "${GREEN}${SERVICE} fechado!${NC}"
    read -r -p "Enter para continuar..." _
}

close_port() {
    local SERVICE_FILE SERVICE NAME LABEL CHOICE INDEX=0 SELECTED=""
    local -a ACTIVE_SERVICES=() ACTIVE_LABELS=()

    while true; do
        ACTIVE_SERVICES=()
        ACTIVE_LABELS=()
        for SERVICE_FILE in "${SYSTEMD_DIR}"/proxy-*.service; do
            [[ -f "$SERVICE_FILE" ]] || continue
            SERVICE=$(basename "$SERVICE_FILE")
            systemctl is-active --quiet "$SERVICE" 2>/dev/null || continue
            NAME=${SERVICE#proxy-}
            NAME=${NAME%.service}
            case "$NAME" in
                integrated) LABEL="INTEGRADO" ;;
                443) LABEL="XHTTP (443)" ;;
                *) LABEL="PORTA ${NAME}" ;;
            esac
            ACTIVE_SERVICES+=("$SERVICE")
            ACTIVE_LABELS+=("$LABEL")
        done

        clear
        show_banner
        echo ""
        box_top
        box_line "${WHITE}${BOLD}Fechar porta aberta${NC}"
        box_mid
        if ((${#ACTIVE_SERVICES[@]} == 0)); then
            box_line "${YELLOW}Nenhum serviço Azpro ativo.${NC}"
        else
            for ((INDEX=0; INDEX<${#ACTIVE_SERVICES[@]}; INDEX++)); do
                printf -v CHOICE '%02d' "$((INDEX + 1))"
                box_line "${WHITE}[${CHOICE}]${NC} - ${ACTIVE_LABELS[$INDEX]}"
            done
        fi
        box_mid
        box_line "${WHITE}[00]${NC} - VOLTAR"
        box_bottom
        echo ""
        read -r -p "Escolha a porta para fechar: " CHOICE
        [[ "$CHOICE" == "00" || "$CHOICE" == "0" || -z "$CHOICE" ]] && return
        if [[ "$CHOICE" =~ ^[0-9]+$ ]] && ((10#$CHOICE >= 1 && 10#$CHOICE <= ${#ACTIVE_SERVICES[@]})); then
            SELECTED=${ACTIVE_SERVICES[$((10#$CHOICE - 1))]}
            close_service "$SELECTED"
        else
            echo -e "${RED}Opção inválida.${NC}"
            sleep 1
        fi
    done
}

manage_special_service() {
    local TITLE="$1"
    local SERVICE="$2"
    local OPEN_FUNCTION="$3"
    local ACTION

    while true; do
        clear
        show_banner
        echo ""
        box_top
        box_line "${WHITE}${BOLD}${TITLE}${NC}"
        box_mid
        box_line "${WHITE}[01]${NC} - ABRIR / CONFIGURAR"
        box_line "${YELLOW}[02]${NC} - REINICIAR PORTA"
        box_line "${RED}[03]${NC} - FECHAR PORTA"
        box_mid
        box_line "${WHITE}[00]${NC} - VOLTAR"
        box_bottom
        echo ""
        read -r -p "Escolha uma opção: " ACTION

        case "$ACTION" in
            01|1)
                "$OPEN_FUNCTION"
                ;;
            02|2)
                if systemctl restart "$SERVICE" 2>/dev/null; then
                    echo -e "${GREEN}${TITLE} reiniciado!${NC}"
                else
                    echo -e "${RED}Não foi possível reiniciar ${SERVICE}.${NC}"
                fi
                read -r -p "Enter para continuar..." _
                ;;
            03|3)
                systemctl stop "$SERVICE" 2>/dev/null
                systemctl disable "$SERVICE" 2>/dev/null
                rm -f "${SYSTEMD_DIR}/${SERVICE}"
                systemctl daemon-reload
                echo -e "${GREEN}${TITLE} fechado!${NC}"
                read -r -p "Enter para continuar..." _
                ;;
            00|0)
                return
                ;;
            *)
                ;;
        esac
    done
}

update_from_github() {
    local CONFIRM="" UPDATE_SCRIPT=""
    clear
    show_banner
    echo ""
    box_top
    box_line "${WHITE}${BOLD}Atualizar Azpro pelo GitHub${NC}"
    box_mid
    box_line "A atualização recompila os binários."
    box_line "Os serviços podem reiniciar durante o processo."
    box_bottom
    echo ""
    read -r -p "Continuar com a atualização? (s/N): " CONFIRM
    [[ "$CONFIRM" != "s" && "$CONFIRM" != "S" ]] && return

    if [[ "$EUID" -ne 0 ]]; then
        echo -e "${RED}Execute o menu como root para atualizar.${NC}"
        read -r -p "Enter para continuar..." _
        return
    fi
    if ! command -v curl >/dev/null 2>&1; then
        echo -e "${RED}curl não está instalado; não foi possível buscar a atualização.${NC}"
        read -r -p "Enter para continuar..." _
        return
    fi

    UPDATE_SCRIPT=$(mktemp /tmp/azpro-update.XXXXXX.sh) || {
        echo -e "${RED}Não foi possível criar arquivo temporário.${NC}"
        read -r -p "Enter para continuar..." _
        return
    }
    echo -e "${CYAN}Baixando instalador mais recente do GitHub...${NC}"
    if ! curl -fsSL --connect-timeout 15 --max-time 60 \
        "https://raw.githubusercontent.com/Jbpro2/Azpro/main/install.sh" \
        -o "$UPDATE_SCRIPT"; then
        rm -f "$UPDATE_SCRIPT"
        echo -e "${RED}Falha ao baixar o instalador. Nada foi atualizado.${NC}"
        read -r -p "Enter para continuar..." _
        return
    fi

    if ! grep -q 'REPO_URL="https://github.com/Jbpro2/Azpro.git"' "$UPDATE_SCRIPT"; then
        rm -f "$UPDATE_SCRIPT"
        echo -e "${RED}O arquivo baixado não passou na validação do repositório.${NC}"
        read -r -p "Enter para continuar..." _
        return
    fi

    bash "$UPDATE_SCRIPT"
    local UPDATE_RESULT=$?
    rm -f "$UPDATE_SCRIPT"
    if [[ $UPDATE_RESULT -eq 0 ]]; then
        echo -e "${GREEN}Atualização concluída.${NC}"
    else
        echo -e "${RED}A atualização falhou; confira as mensagens acima.${NC}"
    fi
    read -r -p "Enter para voltar ao menu..." _
}

remove_owned_link() {
    local LINK="$1"
    local TARGET="$2"
    local CURRENT
    [[ -L "$LINK" ]] || return 0
    CURRENT=$(readlink -- "$LINK") || return 0
    [[ "$CURRENT" == "$TARGET" ]] && rm -f -- "$LINK"
}

uninstall_proxy() {
    local CONFIRM SERVICE_FILE SERVICE STATE BIN FAILED=0
    local -a OWNED_SERVICES=()
    local -a BINARIES=("$LKPROXY" "$LKPROXY_XHTTP" "$LKPROXY_INTEGRATED" "/opt/mpro/proxy-websocket")

    clear
    show_banner
    echo ""
    box_top
    box_line "${RED}${BOLD}DESINSTALAR Azpro${NC}"
    box_mid
    box_line "Para serviços e remove binários do proxy."
    box_line "Remove certificados TLS de /opt/mpro."
    box_line "Preserva SSH, Rust, OpenSSL e demais arquivos."
    box_mid
    echo ""

    if [[ "$EUID" -ne 0 ]]; then
        echo -e "${RED}Execute azpro como root para desinstalar.${NC}"
        read -r -p "Enter para voltar..." _
        return 1
    fi
    if ! command -v systemctl >/dev/null 2>&1; then
        echo -e "${RED}systemctl não encontrado; nada foi removido.${NC}"
        read -r -p "Enter para voltar..." _
        return 1
    fi

    echo -e "${YELLOW}Para desinstalar o proxy, digite: s${NC}"
    echo -e "${YELLOW}Para cancelar a desinstalação, digite: n${NC}"
    read -r -p "Escolha (s/n): " CONFIRM
    if [[ "$CONFIRM" != "s" && "$CONFIRM" != "S" ]]; then
        echo -e "${YELLOW}Desinstalação cancelada.${NC}"
        read -r -p "Enter para voltar..." _
        return 0
    fi

    for SERVICE_FILE in "${SYSTEMD_DIR}"/proxy-*.service; do
        [[ -f "$SERVICE_FILE" ]] || continue
        if grep -Eq '^[[:space:]]*ExecStart=.*/opt/mpro/(proxy|proxy-xhttp|proxy-integrated|proxy-websocket)([[:space:]]|$)' "$SERVICE_FILE"; then
            OWNED_SERVICES+=("$(basename "$SERVICE_FILE")")
        fi
    done

    for SERVICE in "${OWNED_SERVICES[@]}"; do
        systemctl stop "$SERVICE" >/dev/null 2>&1 || true
        STATE=$(systemctl show --property=ActiveState --value "$SERVICE" 2>/dev/null) || STATE="unknown"
        case "$STATE" in
            inactive|failed) ;;
            *)
                echo -e "${RED}${SERVICE} não parou com segurança (estado: ${STATE}); binários mantidos.${NC}"
                FAILED=1
                ;;
        esac
    done
    if [[ "$FAILED" -ne 0 ]]; then
        read -r -p "Resolva os serviços ativos e tente novamente. Enter para voltar..." _
        return 1
    fi

    for SERVICE in "${OWNED_SERVICES[@]}"; do
        if ! systemctl disable "$SERVICE" >/dev/null 2>&1; then
            echo -e "${RED}Falha ao desabilitar ${SERVICE}; arquivos do proxy foram mantidos.${NC}"
            read -r -p "Enter para voltar..." _
            return 1
        fi
    done
    for SERVICE in "${OWNED_SERVICES[@]}"; do
        rm -f -- "${SYSTEMD_DIR}/${SERVICE}"
    done
    if ! systemctl daemon-reload; then
        echo -e "${RED}Falha ao recarregar systemd; arquivos do proxy foram mantidos.${NC}"
        read -r -p "Enter para voltar..." _
        return 1
    fi

    for BIN in "${BINARIES[@]}"; do
        pkill -f "^${BIN}([[:space:]]|$)" >/dev/null 2>&1 || true
    done
    sleep 1
    for BIN in "${BINARIES[@]}"; do
        if pgrep -f "^${BIN}([[:space:]]|$)" >/dev/null 2>&1; then
            echo -e "${RED}Ainda há um processo ${BIN} ativo; arquivos do proxy foram mantidos.${NC}"
            read -r -p "Enter para voltar..." _
            return 1
        fi
    done

    remove_owned_link "/usr/local/bin/azpro" "/opt/mpro/menu"
    remove_owned_link "/usr/local/bin/wzpro" "/opt/mpro/menu"
    remove_owned_link "/usr/local/bin/wspro" "/opt/mpro/menu"
    remove_owned_link "/usr/local/bin/mpro" "/opt/mpro/menu"
    remove_owned_link "/usr/local/bin/mpro-xhttp" "$LKPROXY_XHTTP"
    remove_owned_link "/usr/local/bin/mpro-integrated" "$LKPROXY_INTEGRATED"
    rm -f -- "${BYPASS_DIR}"/bypass-http-ssh-*.enabled \
        "${BYPASS_DIR}"/multiprotocol-*.enabled
    if ! rm -f -- "$LKPROXY" "$LKPROXY_XHTTP" "$LKPROXY_INTEGRATED" \
        "/opt/mpro/proxy-websocket" "/opt/mpro/menu" "/opt/mpro/cert.pem" \
        "/opt/mpro/key.pem" "$LEGACY_MULTIPROTOCOL_FLAG"; then
        echo -e "${RED}Falha ao remover alguns arquivos; revise /opt/mpro.${NC}"
        read -r -p "Enter para voltar..." _
        return 1
    fi
    rm -rf -- "/var/cache/azpro/cargo-target" "/var/cache/wzpro/cargo-target"
    rmdir /var/cache/azpro /var/cache/wzpro 2>/dev/null || true
    rmdir /opt/mpro 2>/dev/null || true

    echo -e "${GREEN}Azpro desinstalado. SSH e dependências do sistema foram preservados.${NC}"
    echo -e "${YELLOW}Arquivos não reconhecidos em /opt/mpro, se houver, foram preservados.${NC}"
    sleep 3
    exit 0
}

show_menu() {
    clear
    show_banner
    echo ""
    box_top
    show_execution_status
    box_mid
    show_active_ports
    box_mid
    box_line "${WHITE}[01]${NC} ${RED}- ABRIR PORTA${NC}"
    box_line "${WHITE}[02]${NC} ${RED}- FECHAR PORTA ABERTA${NC}"
    box_line "${WHITE}[03]${NC} ${RED}- REINICIAR PORTA${NC}"
    box_line "${WHITE}[04]${NC} ${RED}- XHTTP + SSL TUNNEL${NC}"
    box_line "${WHITE}[05]${NC} ${RED}- PROTOCOLO INTEGRADO${NC}"
    box_line "${WHITE}[06]${NC} ${RED}- ATUALIZAR PELO GITHUB${NC}"
    box_line "${WHITE}[07]${NC} ${RED}- BYPASS SSH_PROXY (HTTP)${NC}"
    box_line "${WHITE}[08]${NC} ${RED}- DESINSTALAR PROXY${NC}"
    box_line ""
    box_line "${WHITE}[00]${NC} - SAIR"
    box_bottom
    echo ""
    echo -n "Escolha uma opção: "
}

while true; do
    show_menu
    read OPTION
    case $OPTION in
        01|1) open_port ;;
        02|2) close_port ;;
        03|3) restart_port ;;
        04|4) manage_special_service "XHTTP + SSL TUNNEL" "proxy-443.service" open_xhttp ;;
        05|5) manage_special_service "PROTOCOLO INTEGRADO" "proxy-integrated.service" open_integrated ;;
        06|6) update_from_github ;;
        07|7) manage_bypass ;;
        08|8) uninstall_proxy ;;
        00|0) echo "👋 Saindo..."; exit 0 ;;
        *) sleep 1 ;;
    esac
done
