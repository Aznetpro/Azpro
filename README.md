# Mpro v2.0 - com suporte xHTTP (SplitHTTP)

Proxy multi-protocolo em Rust com suporte exclusivo para o protocolo xHTTP (SplitHTTP) na porta 443, compatível com o cliente **XHTTP-DEMO**.

## O que mudou na v2.0

| Recurso | v1.0 | v2.0 |
|---------|------|------|
| xHTTP/SplitHTTP exclusivo | Não | Sim (opção 04) |
| TLS termination | Passthrough | Terminação com rustls |
| Porta 443 automática | Manual | Automática (opção 04) |
| Certificados auto-assinados | Não | Gerados automaticamente |
| UDP + QUIC na 443 | Manual | Auto-ativados |
| SocksRevive compatibilidade | Não | Sim |

## Instalação

```bash
# Clonar ou copiar os arquivos para o servidor
git clone https://github.com/Jbpro2/Azpro.git
cd Azpro

# Substituir os arquivos pelo v2.0
cp menu.sh menu_original.sh.backup
# Copiar os novos arquivos para o repositório

# Compilar e instalar
chmod +x install.sh
./install.sh
```

## Uso

```bash
azpro   # Único comando para abrir o menu deste proxy
```

### Opção 01 — Detecção multiprotocolo

A opção **[01] ABRIR PORTA** detecta HTTP, SOCKS4/SOCKS4a, SOCKS5, SECURITY e WebSocket automaticamente na porta TCP escolhida. HTTP e SECURITY usam o status personalizado informado no menu; WebSocket e SOCKS usam os códigos definidos por seus próprios protocolos. SOCKS4/4a, SOCKS5 e WebSocket encaminham somente ao SSH local (`127.0.0.1:22`); SOCKS não permite acesso arbitrário a outros destinos e a opção não cria um proxy aberto. HTTP mantém o fluxo de túnel implementado pelo Azpro, não é um proxy HTTP geral. TLS/HTTPS direto não está incluído na opção 01; para xHTTP/TLS, use a opção 04.

No menu, escolha **[06] ATUALIZAR PELO GITHUB** para baixar a versão mais recente do branch `main`, recompilar os binários e atualizar o menu. Os serviços Azpro que estavam ativos são parados durante a troca e iniciados novamente ao final; as configurações systemd e portas desligadas são preservadas. O instalador reaproveita o cache de compilação em `/var/cache/azpro/cargo-target`, instala dependências somente quando faltam e usa paralelismo limitado pela memória disponível. A primeira compilação ainda pode demorar mais; o tempo varia conforme VPS, rede e cache.

### Menu v2.0

```
╔══════════════════════════════════╗
║       Mpro Menu Free v2.0     ║
╠══════════════════════════════════╣
║                                  ║
║ [01] - ABRIR PORTA               ║
║ [02] - FECHAR PORTA              ║
║ [03] - REINICIAR PORTA           ║
║ [04] - xHTTP SPLITHTTP (443)    ║
║                                  ║
║ [00] - SAIR                      ║
╚══════════════════════════════════╝
```

### Opção 04 - xHTTP SplitHTTP (Porta 443)

Esta opção é **exclusiva** para o protocolo xHTTP (SplitHTTP) e funciona assim:

1. **Porta fixa 443** - Não pode ser alterada
2. **TLS obrigatório** - Certificado auto-assinado gerado automaticamente
3. **SSH only** - Tunnel para SSH local (127.0.0.1:22)
4. **UDP + QUIC** - Ativados automaticamente
5. **Status HTTP** - Cabeçalho retornado ao cliente (padrão: @Mpro)

### Fluxo do xHTTP

```
Cliente (SocksRevive)          Servidor (Mpro)
       │                              │
       ├── TCP connect :443 ──────────┤
       ├── TLS ClientHello ───────────┤
       │                              ├── TLS handshake (rustls)
       │                              ├── TLS ServerHello
       │                              ├── TLS decodificado
       │                              │
       ├── HTTP/2 GET /ssh/{id} ─────┤
       │                              ├── Sessão criada
       │                              ├── Conecta SSH backend
       │                              ├── Streaming chunked
       │                              │
       │◄── HTTP 200 + chunks ────────┤ (dados SSH do servidor)
       │                              │
       ├── HTTP/2 POST /ssh/{id}/0 ──┤ (dados SSH do cliente)
       │                              ├── Encaminha para SSH backend
       │                              │
       ├── HTTP/2 POST /ssh/{id}/1 ──┤
       │                              ├── Encaminha para SSH backend
```

### Configuração no SocksRevive-XHTTP-DEMO

| Campo | Valor |
|-------|-------|
| Server | IP do servidor Mpro |
| Port | 443 |
| SNI | Qualquer domínio (trust-all) |
| XHTTP Host | IP do servidor ou vazio |
| XHTTP Path | /ssh |
| XHTTP TLS | HABILITADO |
| Username | Usuário SSH |
| Password | Senha SSH |

## Flags do Binário

```
-p PORTA       Porta TCP (padrão: 80)
-s STATUS      Status HTTP (padrão: @Mpro)
-t             Habilitar TLS
-ssh           SSH only (tunnel para 127.0.0.1:22)
-u             Habilitar UDP
-q             Habilitar QUIC
-x             xHTTP mode (força porta 443, TLS, SSH only)
```

## Notas sobre TLS

O TLS na v2.0 usa **terminação local** com `rustls`:

1. O certificado e chave são gerados em `/opt/mpro/`
2. O proxy faz handshake TLS completo com o cliente
3. Os dados decodificados são roteados para o handler correto
4. Para xHTTP: os dados HTTP/2 ficam visíveis após TLS

## Troubleshooting

```bash
# Ver logs
journalctl -u proxy-443.service -f

# Verificar SSH
systemctl status ssh

# Testar TLS
openssl s_client -connect 127.0.0.1:443

# Verificar porta
ss -tlnp | grep 443
```

## Requisitos

- Rust 1.70+
- systemd
- OpenSSH Server (rodando na porta 22)
- OpenSSL (para certificados)
- Porta 443 aberta no firewall

## Opção 07 — BYPASS SSH_PROXY (HTTP)

A opção **[07] BYPASS SSH_PROXY (HTTP)** lista as portas numéricas Azpro ativas e permite ligar/desligar o BYPASS individualmente em cada porta. A porta selecionada é reiniciada e as conexões atuais caem. A opção 01 e suas perguntas/configurações permanecem iguais; as opções 04 e 05 não são alteradas.

O BYPASS é destinado ao modo **SSH_PROXY** do aplicativo com payload HTTP. Para uma conexão HTTP válida, o servidor envia `101 Switching Protocols`, lê o payload e depois envia `101 <status>` e `200 <status>`, encaminhando o túnel ao SSH local da VPS (`127.0.0.1:22`). Esta sequência é específica para clientes compatíveis e não é um handshake HTTP/WebSocket padrão. O BYPASS não atua como proxy aberto nem como dispatcher de SOCKS5, TLS, WebSocket ou SECURITY. Conexões SSH diretas sem payload são encaminhadas ao SSH local sem resposta HTTP; outros protocolos não são aceitos nesse modo. Com o BYPASS desligado, a opção 01 mantém o handler legado.

O marcador BYPASS é individual por porta (`/opt/mpro/bypass-http-ssh-<porta>.enabled`). Marcadores do antigo multiprotocolo são ignorados pelo servidor e removidos pela opção de desinstalação. Usuário e senha SSH continuam sendo enviados pelo aplicativo e validados pelo servidor OpenSSH. BHTTP não está implementado.

## Opção 08 — Desinstalar Proxy

A opção **[08] DESINSTALAR PROXY** exige execução como root e confirmação em `Deseja Desinstalar Proxy? (s/n):`; somente `s` ou `S` confirma. Ela para e desabilita somente os serviços systemd cuja linha `ExecStart` aponta para os binários Azpro em `/opt/mpro`, remove os links instalados em `/usr/local/bin` quando ainda apontam para o Azpro e apaga os binários, menu, certificados TLS e marcadores BYPASS/legados conhecidos. Não remove OpenSSH, Rust, OpenSSL, regras de firewall nem arquivos desconhecidos que estejam em `/opt/mpro`; se sobrarem arquivos, o diretório é preservado.
