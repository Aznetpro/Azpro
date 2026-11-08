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
git clone https://github.com/Lacasx1/Azpro.git
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

### Opção 01 — HTTP/TLS compatível com AWProxy

Com a opção 7 desligada, **[01] ABRIR PORTA** mantém o fluxo HTTP legado compatível com a opção 1 do AWProxy; nesse modo não despacha SOCKS4/5, SECURITY, WebSocket ou SSH_DIRECT. Ative a opção 7 para usar esses handlers multiprotocolo na porta. O menu pergunta HTTPS/TLS ao abrir a porta; `s` ativa o passthrough TLS do Azpro e `n` o desativa. Essa terminação não ocorre localmente: o tráfego TLS é encaminhado sem descriptografia. O `-t` do AWProxy existe no serviço, mas o `main.rs` atual do AWProxy não o consulta; no Azpro ele é respeitado para manter funcional a escolha HTTP/TLS solicitada. Da mesma forma, o Azpro respeita `-ssh` para forçar o backend SSH; o `main.rs` atual do AWProxy ignora essa flag.

Na opção 1, o handler original permanece `HTTP/1.1 101 <status>`, uma leitura de até 1024 bytes e `HTTP/1.1 200 <status>`; não envia o segundo `101` nem retransmite aquele buffer inicial ao backend. A sequência `101`, `101`, `200` com o mesmo texto e o encaminhamento do preâmbulo só são usados quando a opção 7 está ativa para a porta. O texto `<status>` vem da configuração do serviço, por exemplo `Aznet`. Se “somente SSH” estiver ativa, encaminha ao SSH local (`127.0.0.1:22`); caso contrário, a seleção legada inspeciona os próximos bytes com `peek`, como no código original, e escolhe SSH ou VPN local (`127.0.0.1:1194`). O padrão do Azpro é `@Azpro`.

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

## Opção 07 — Multiprotocolo + Status HTTP + TCP

A opção **[07] MULTIPROTOCOLO + STATUS + TCP** lista as portas numéricas Azpro ativas e permite ativar/desativar o dispatcher de protocolos em cada porta. Ao alternar, a porta é reiniciada e as conexões atuais caem. As opções 01–06 permanecem separadas; se o modo estiver desligado, a opção 01 mantém o fluxo HTTP legado.

Com o modo ligado, o serviço detecta SOCKS4/SOCKS4a, TLS ClientHello, WebSocket RFC 6455, SECURITY, HTTP e SSH direto. Para HTTP/SSH_PROXY, envia um único `101 <status>`, remove somente os headers do preâmbulo HTTP, envia `200 <status>` e mantém o relay bidirecional com SSH local (`127.0.0.1:22`). A opção 7 agora usa a mesma sequência de status do fluxo HTTP legado e não repete o `101`. Bytes do túnel que cheguem junto com o cabeçalho são preservados e encaminhados ao `sshd`; o cabeçalho HTTP não é encaminhado. O texto de status é o configurado na flag `-s`. WebSocket usa os códigos/reason phrases exigidos pelo protocolo. Dados TCP desconhecidos são encaminhados pelo fallback local. SOCKS4/5 só permitem CONNECT à porta 22; o cliente não pode escolher host ou porta arbitrários.

O estado é individual por porta, no marcador `/opt/mpro/multiprotocol-<porta>.enabled`. Ativar uma porta que não tenha OpenSSH na 22 e, quando aplicável, VPN/OpenVPN na 1194 fará o respectivo protocolo falhar; verifique os serviços locais antes de testar. O listener UDP existente apenas registra datagramas e QUIC continua sem listener funcional; a opção 7 ativa despacho TCP, não transforma esses componentes em serviços UDP/QUIC.

## Opção 08 — Desinstalar Proxy

A opção **[08] DESINSTALAR PROXY** exige execução como root. O menu explica: digite `s` para desinstalar ou `n` para cancelar; somente `s` ou `S` confirma. Ela para e desabilita somente os serviços systemd cuja linha `ExecStart` aponta para os binários Azpro em `/opt/mpro`, remove os links instalados em `/usr/local/bin` quando ainda apontam para o Azpro e apaga os binários, menu, certificados TLS e marcadores BYPASS/legados conhecidos. Não remove OpenSSH, Rust, OpenSSL, regras de firewall nem arquivos desconhecidos que estejam em `/opt/mpro`; se sobrarem arquivos, o diretório é preservado.
