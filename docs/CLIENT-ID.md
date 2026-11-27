# Where to get the Spotify Client ID

`flowpause` needs the Client ID of an app that you create yourself in the Spotify developer dashboard. The app is yours alone, it is free, and it takes about 5 minutes to create. The client secret is not used.

## Before you start

- Use the same Spotify **Premium** account that plays on the Echo and on the PC. Since February 2026, Development Mode requires the app owner to have Premium.
- The first time, the dashboard asks you to accept the developer terms and may ask you to confirm your email.

## Step by step

1. Open <https://developer.spotify.com/dashboard> and log in with your Spotify account.
2. Click **Create app**.
3. Fill in the form:

   | Field | What to enter |
   | --- | --- |
   | App name | `flowpause` (or another name) |
   | App description | Any text, for example `Pauses Spotify during dictation` |
   | Website | Leave empty |
   | Redirect URIs | `http://127.0.0.1:8888/callback`, then click **Add** |
   | Which API/SDKs are you planning to use? | Check **Web API** |

4. Accept the terms and click **Save**.
5. On the app page, click **Settings**.
6. Under **Basic Information**, copy the **Client ID** value: 32 characters, letters `a` to `f` and digits. Do not click **View client secret**; the secret is not used.

## Using the Client ID

Run the install script and paste the Client ID when it asks:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```

Or pass it directly:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -ClientId <your client id>
```

The script saves the value in the user variable `FLOWPAUSE_CLIENT_ID`. The Client ID is not a secret, but it does not need to go into git.

## If login fails

| Message in the browser | Cause | What to do |
| --- | --- | --- |
| `INVALID_CLIENT: Invalid client` | Wrong or incomplete Client ID | Copy it again from **Settings > Basic Information** |
| `INVALID_CLIENT: Invalid redirect URI` | The app's Redirect URI is not exactly `http://127.0.0.1:8888/callback` | Fix it in **Settings > Redirect URIs**. Use `127.0.0.1`, not `localhost` (Spotify does not accept `localhost`) |
| User not registered / not authorized | The account that logged in is not on the app's list | In **User Management**, add the name and email of your Spotify account |
