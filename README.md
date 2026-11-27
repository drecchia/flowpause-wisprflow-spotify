# flowpause

Utilitário de bandeja para Windows que pausa o Spotify quando o Wispr Flow começa a ouvir e retoma no mesmo dispositivo quando o ditado termina. A especificação está em [`docs/spec.md`](docs/spec.md).

**Estado atual: fases 1 e 2.** O comportamento final roda em modo console (`flowpause run`). Ainda não há bandeja, início com o Windows nem arquivo de configuração (fase 3). Resultados: [`docs/FASE0-RESULTADOS.md`](docs/FASE0-RESULTADOS.md) e [`docs/FASE1-2-RESULTADOS.md`](docs/FASE1-2-RESULTADOS.md).

## Requisitos

- Windows 11, toolchain Rust `x86_64-pc-windows-msvc`.
- Conta Spotify Premium.

## 1. Criar o app no painel do Spotify

Siga [`docs/CLIENT-ID.md`](docs/CLIENT-ID.md): como criar o app, onde copiar o Client ID e o que fazer se o login falhar.

## 2. Instalação com script (recomendado)

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```

O script primeiro avisa sobre o Kaspersky e espera Enter. Ele pode pôr em quarentena os executáveis da compilação, e o build falha com "Acesso negado". Adicione exclusões para a pasta `target\` do projeto e para `%USERPROFILE%\.cargo\bin\flowpause.exe`, ou pause a proteção. Depois o script pede o Client ID, grava `FLOWPAUSE_CLIENT_ID` nas variáveis do usuário, roda `cargo build --release` e `cargo install --path . --locked`, e confere se o `flowpause.exe` instalado executa.

Para instalar à mão, siga as seções 2a e 3.

## 2a. Definir `FLOWPAUSE_CLIENT_ID`

Só na sessão atual do PowerShell:

```powershell
$env:FLOWPAUSE_CLIENT_ID = "<seu client id>"
```

Permanente para o usuário (vale em terminais abertos depois):

```powershell
setx FLOWPAUSE_CLIENT_ID "<seu client id>"
```

## 3. Compilar e instalar

```powershell
cargo build --release
cargo install --path .
```

`cargo install` copia `flowpause.exe` para `%USERPROFILE%\.cargo\bin`, que já está no `PATH`. Sem instalar, use `.\target\release\flowpause.exe` no lugar de `flowpause`.

## 4. Comandos

| Comando | Chamada |
| --- | --- |
| `flowpause login` | Authorization Code com PKCE. Abre o navegador e recebe o callback em `127.0.0.1:8888` |
| `flowpause status` | `GET /v1/me/player` |
| `flowpause devices` | `GET /v1/me/player/devices` |
| `flowpause pause` | `PUT /v1/me/player/pause` |
| `flowpause play --device <id>` | `PUT /v1/me/player/play?device_id=<id>` |
| `flowpause transfer --device <id>` | `PUT /v1/me/player` com `{"device_ids":["<id>"],"play":true}` |
| `flowpause watch [--match wispr]` | Só o detector. Imprime `ditado iniciou` / `ditado terminou` com horário. Não usa o Spotify |
| `flowpause run [--resume-delay-ms 2000] [--max-pause-secs 300] [--match wispr]` | Detector + controlador: pausa durante o ditado e retoma no mesmo dispositivo. Ctrl+C para sair |

Cada comando imprime a chamada, o status HTTP, o tempo da chamada em ms e o corpo do erro, se houver. Se o access token precisou ser renovado, a renovação aparece numa linha à parte, com o próprio tempo; o número da chamada não inclui a renovação. O código de saída é 0 para resposta 2xx e 1 para o resto.

Os tokens ficam no Gerenciador de Credenciais do Windows (Credenciais do Windows > Credenciais Genéricas), com os nomes `spotify-refresh-token.flowpause` e `spotify-access-token.flowpause`. Para apagar o login:

```powershell
cmdkey /delete:spotify-refresh-token.flowpause
cmdkey /delete:spotify-access-token.flowpause
```

### `watch` e `run`

O detector lê `HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone`, onde o Windows registra quando cada app abre e fecha o microfone. `--match` escolhe as subchaves pelo nome (padrão `wispr`, sem diferenciar maiúsculas). Não há polling: o programa espera a notificação de mudança do registro.

`run` imprime, com horário, cada evento de ditado, cada transição de estado (`Ocioso`, `Pausado`, `SemRetomada`, `Aguardando`) e cada chamada HTTP com status e ms. Regras:

- Antes de pausar, consulta o estado e só pausa se houver música tocando. A fase 0 não testou o 403 ao pausar o que já está pausado, por isso não pausa às cegas.
- Retoma só no dispositivo onde pausou, `--resume-delay-ms` depois do fim do ditado.
- Não retoma se, na hora, a música já estiver tocando, se o dispositivo ativo for outro ou se a pausa passou de `--max-pause-secs`.
- Um novo ditado durante a espera cancela a retomada.
- Em 404 no play, tenta uma transferência para o mesmo dispositivo.
- Ctrl+C com a música pausada pelo programa: tenta retomar antes de sair.

## 5. Roteiro de teste da fase 0

Hipóteses:

- **H1:** o Echo Dot aceita `pause` e `play` pela Web API (não é dispositivo restrito).
- **H2:** pausar quando já está pausado devolve 403.

Anote cada resultado em `docs/FASE0-RESULTADOS.md`.

1. `flowpause login`
2. Com música tocando no Echo: `flowpause status`. Anotar `id` e `is_restricted`.
3. `flowpause pause`. Anotar status HTTP e ms; conferir se o Echo parou e estimar o tempo até o silêncio.
4. `flowpause pause` de novo. Anotar o status HTTP (valida H2).
5. `flowpause play --device <id>`. Conferir se voltou no Echo.
6. Pausar (`flowpause pause`), esperar 5 min, `flowpause devices`. O Echo ainda aparece? Depois `flowpause play --device <id>`; se der 404, `flowpause transfer --device <id>`.
7. Repetir 2 a 5 com a música no Spotify do PC.

## 6. Roteiro de teste das fases 1 e 2

Anote cada resultado em `docs/FASE1-2-RESULTADOS.md`.

1. `flowpause watch` e 5 ditados: cada um gera exatamente um `ditado iniciou` e um `ditado terminou`.
2. `flowpause run` com música no Echo: ditar. A música para e volta no Echo cerca de 2 s depois.
3. O mesmo com música no Spotify do PC.
4. Sem música: ditar. Nada começa a tocar.
5. Três ditados com menos de 2 s entre eles: uma pausa, uma retomada.
6. Ditar e dar play manual durante a pausa: o programa não envia comando.
7. Ditar com música no Echo e trocar para o PC durante a pausa: não retoma no Echo.
