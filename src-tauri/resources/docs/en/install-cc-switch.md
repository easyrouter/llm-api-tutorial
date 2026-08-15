# Install CC Switch

CC Switch is a small open-source desktop app (MIT licence) that manages provider configuration for Codex CLI and Claude Code in one place: you enter the service gateway once in its interface, it writes the configuration to the right place for each tool, and later you can switch between providers with one click. Because the company requires multi-provider switching, CC Switch is a **required** component.

## How the tool helps

1. It fetches the latest release from CC Switch's official release channel and picks the installer for your OS and CPU.
2. It downloads over https into this tool's own cache folder and, when the release publishes a SHA-256 checksum, verifies the file before handing it over.
3. Click "Open installer" when the download finishes; CC Switch's own installer does the rest.

Only download from the link the tool provides or from the company intranet. Do not click random search results — fake download sites have appeared in the past.

## Windows

1. Download the `.exe` (NSIS) installer or the portable build.
2. Double-click it. If the blue **"Windows protected your PC"** dialog appears, click **"More info" → "Run anyway"** (see fault G).
3. After installation, start CC Switch once to make sure it opens.

## macOS

1. Download the `.dmg`, open it and drag CC Switch into the Applications folder.
2. If the first launch says **"cannot be opened because the developer cannot be verified"** or **"is damaged"**: open **System Settings → Privacy & Security**, scroll to the bottom and click **"Open Anyway"**. If the button is not there, run this in Terminal:

   ```
   xattr -d com.apple.quarantine /Applications/CC\ Switch.app
   ```

   and open the app again (see fault G).

## Afterwards

Back in the tool, click "Re-check". Once the CC Switch row shows "Pass" you can move on to "Configure". You will do the configuration inside CC Switch's own window while this tool shows the values to enter for each step.
