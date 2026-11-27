# flowpause

Utilitário de bandeja para Windows que pausa o Spotify quando o Wispr Flow começa a ouvir e retoma no mesmo dispositivo quando o ditado termina. A especificação está em [`docs/spec.md`](docs/spec.md).

**Estado atual: fase 0.** Só existe uma CLI mínima que valida se o Echo Dot aceita `pause` e `play` pela Spotify Web API. Os resultados vão em [`docs/FASE0-RESULTADOS.md`](docs/FASE0-RESULTADOS.md).

## Requisitos

- Windows 11, toolchain Rust `x86_64-pc-windows-msvc`.
- Conta Spotify Premium.

## 1. Criar o app no painel do Spotify

1. Acesse <https://developer.spotify.com/dashboard> com a conta Premium e clique em **Create app**.
2. Preencha nome e descrição livres.
3. Em **Redirect URIs**, adicione exatamente `http://127.0.0.1:8888/callback`.
4. Em **Which API/SDKs are you planning to use?**, marque **Web API**.
5. Aceite os termos e salve.
6. Abra **Settings** do app e copie o **Client ID**. O client secret não é usado.

O app fica em Development Mode, o que basta para uso pessoal. Se o login falhar dizendo que o usuário não está registrado, adicione o e-mail da sua conta Spotify em **User Management**.

## 2. Definir `FLOWPAUSE_CLIENT_ID`

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

Cada comando imprime a chamada, o status HTTP, o tempo da chamada em ms e o corpo do erro, se houver. Se o access token precisou ser renovado, a renovação aparece numa linha à parte, com o próprio tempo; o número da chamada não inclui a renovação. O código de saída é 0 para resposta 2xx e 1 para o resto.

Os tokens ficam no Gerenciador de Credenciais do Windows (Credenciais do Windows > Credenciais Genéricas), com os nomes `spotify-refresh-token.flowpause` e `spotify-access-token.flowpause`. Para apagar o login:

```powershell
cmdkey /delete:spotify-refresh-token.flowpause
cmdkey /delete:spotify-access-token.flowpause
```

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
