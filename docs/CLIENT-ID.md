# Onde pegar o Client ID do Spotify

O `flowpause` precisa do Client ID de um app que você mesmo cria no painel de desenvolvedor do Spotify. O app é só seu, é gratuito e leva uns 5 minutos para criar. O client secret não é usado.

## Antes de começar

- Use a mesma conta Spotify **Premium** que toca no Echo e no PC. Desde fevereiro de 2026, o Development Mode exige que o dono do app seja Premium.
- Na primeira vez, o painel pede para aceitar os termos de desenvolvedor e pode pedir para confirmar o e-mail.

## Passo a passo

1. Abra <https://developer.spotify.com/dashboard> e entre com a sua conta Spotify.
2. Clique em **Create app**.
3. Preencha o formulário:

   | Campo | O que colocar |
   | --- | --- |
   | App name | `flowpause` (ou outro nome) |
   | App description | Qualquer texto, por exemplo `Pausa o Spotify durante o ditado` |
   | Website | Deixe em branco |
   | Redirect URIs | `http://127.0.0.1:8888/callback` e clique em **Add** |
   | Which API/SDKs are you planning to use? | Marque **Web API** |

4. Aceite os termos e clique em **Save**.
5. Na página do app, clique em **Settings**.
6. Em **Basic Information**, copie o valor de **Client ID**: 32 caracteres, letras de `a` a `f` e números. Não clique em **View client secret**, porque o secret não é usado.

## Usar o Client ID

Rode o script de instalação e cole o Client ID quando ele pedir:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```

Ou passe direto:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -ClientId <seu client id>
```

O script grava o valor na variável de usuário `FLOWPAUSE_CLIENT_ID`. O Client ID não é segredo, mas não precisa ir para o git.

## Se o login falhar

| Mensagem no navegador | Causa | O que fazer |
| --- | --- | --- |
| `INVALID_CLIENT: Invalid client` | Client ID errado ou incompleto | Copie de novo em **Settings > Basic Information** |
| `INVALID_CLIENT: Invalid redirect URI` | A Redirect URI do app não é exatamente `http://127.0.0.1:8888/callback` | Corrija em **Settings > Redirect URIs**. Use `127.0.0.1`, não `localhost` (o Spotify não aceita `localhost`) |
| Usuário não registrado / não autorizado | A conta que fez login não está na lista do app | Em **User Management**, adicione o nome e o e-mail da sua conta Spotify |
