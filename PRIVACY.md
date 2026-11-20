# Privacy Policy for Sora Notch

Last updated: November 20, 2026 (version 1.0.0)

Sora Notch ("Sora") is a desktop companion for Windows and macOS, made by Aqmilo Works. A small notch at the top of your screen shows the time, weather, a calendar, to-dos, alarms, a focus timer, music controls, market prices, a system monitor, a clipboard board, a file tray, file sending and receiving, and the status of AI coding agents. This policy explains what Sora does with your information.

## The short version

* Sora has no accounts, no advertising and no analytics or tracking.
* The Sora project runs no server that receives your content. Your events, to-dos, settings, clipboard board, files and recordings stay on your computer.
* Sora goes online only for the features listed in "Information sent to online services", and only when you use them.
* Sora never sells or shares your data.
* Uninstalling Sora deletes everything it saved.

## Editions

| Edition | What it includes |
|-|-|
| GitHub version for Windows (installer and portable) | Every feature, including the beta features chat and meeting recorder. |
| Microsoft Store version | Every feature except chat and the meeting recorder. Updates come through the Microsoft Store. |
| Mac app | Every feature except chat and the meeting recorder. |

Parts of this policy marked "GitHub version" apply only to the GitHub version for Windows.

## Information stored on your device

Sora saves the following only on your computer, in its app data folder (`%APPDATA%\com.nook.desktop` and `%LOCALAPPDATA%\com.nook.desktop` on Windows, `~/Library/Application Support/com.nook.desktop` on a Mac):

* Your settings (notch size and position, shortcuts, chosen place, preferences, language).
* Calendar events you add and whether you ticked them off. Ticked events are deleted 24 hours after you tick them; unticked events are deleted after their day has passed.
* To-dos, alarms, focus sessions and the market symbols you add.
* Water reminders and screen time per app (app names and time used), only while you switch those widgets on.
* Your place: the city or address details you pick or let Sora find (district, city, province, country and coordinates; never a street, house number or postal code).
* The list of files you place in the Tray and in Send (paths and sizes, not copies of the files).
* **Clipboard board:** text, links and images you copy, for 7 days (items you pin with Keep stay until you remove them). Sora skips what a password manager marks as "do not store". Text found in copied pictures (OCR) is read on your computer and never leaves it.
* **Files you receive** with LocalSend, only after you accept them: saved in your Downloads folder.
* **Meeting recordings** (GitHub version, only if you turn the meeting recorder on): audio of the app or the PC audio you choose and, if you allow it, your microphone, saved in `Documents\Sora Recordings` with the transcripts and summaries made from them.
* **Chat history** (GitHub version, only if you turn chat on): notifications and messages shown in the notch, kept for 30 days and encrypted with a key that Windows protects for your account. You can turn this off, delete single entries or clear all of it.
* **Chat sessions** (GitHub version, only if you link WhatsApp or Telegram): the login session of the account you linked, encrypted on your PC. Unlinking deletes it.

**Deleting your data:** uninstalling Sora deletes all of the above (except files you received or recordings you saved, which are yours). The Windows installer and the Microsoft Store remove it with the app; the portable Windows copy and the Mac app remove it with Settings > General > Uninstall Sora.

## Information read on your device and not sent anywhere

* **Now playing:** the song title, artist, album, artwork and position, read from Windows' media controls or, on a Mac, from Spotify and Music.
* **System monitor:** CPU, GPU, memory, disk and network readings, shown on Home only.
* **Location:** if you choose "Use my location", Sora asks Windows or macOS for your position. Sora's use of it is described below.
* **Files:** adding files to the Tray, converting them, creating thumbnails and reading text from pictures all happen on your computer.
* **AI agent status** (only if you turn it on): Sora receives events from the coding agents you connect (Claude Code, Codex, Gemini CLI, Antigravity and others) through hooks it installs in their settings, with a preview and a backup. It shows their state and asks you to approve actions. Nothing is sent to the Sora project.
* **Notifications of other apps** (GitHub version, only if you turn chat on): Sora reads the notifications that chat apps show (WhatsApp, Telegram, Discord and similar) through Windows' notification access, only to show them in the notch and, if you keep it on, in the chat history. While a chat app is shown in the notch, Windows' own pop-up for it is turned off and put back when Sora quits. You can hide the message text on the notch cards in Settings.

## Information sent to online services

Sora only sends what each feature needs:

|Feature|Service|What is sent|
|-|-|-|
|Weather|Open-Meteo (open-meteo.com)|The latitude and longitude of your place.|
|Place names|OpenStreetMap Nominatim (nominatim.openstreetmap.org), BigDataCloud (bigdatacloud.net)|Your coordinates, to name the district and city, or the place name you search for.|
|Approximate location (only if system location is not available and you use "Use my location")|ipwho.is, ipapi.co|A standard web request; the service sees your IP address and returns an approximate city.|
|Market prices|Yahoo Finance (query1.finance.yahoo.com)|The symbols you added, such as AAPL or BTC-USD, and what you type in the market search.|
|Song lyrics|LRCLIB (lrclib.net), Lyrics.ovh, NetEase Cloud Music, QQ Music|The title, artist, album and length of the song that is playing.|
|Link previews on the clipboard board|The website of the link, or its embed service (for example YouTube, Spotify, X, Reddit, SoundCloud)|A standard request for that link, without cookies. Sora loads the page title, description and image, nothing else.|
|Send to iPhone or Mac (QR code)|Sora's static page (send.soranotch.com and share.soranotch.com), Cloudflare DNS (cloudflare-dns.com)|A random installation name and your computer's local network address, so the page can reach your computer and your phone can trust it. File names and files are never sent.|
|Plan usage (only if AI agent is on)|api.anthropic.com|A request with your own Claude Code login, to show your plan usage. Nothing else.|
|Updates (GitHub version and Mac app)|update.soranotch.com, then GitHub (github.com)|A standard web request to look for a newer version, then the download from the GitHub release. The download is checked against the update information signed by the publisher before it is installed. No identifier is sent.|
|Chat helper updates (GitHub version, chat on)|update.soranotch.com|A standard request to look for a newer helper; a random number decides when your PC gets it. No identifier is sent.|
|WhatsApp and Telegram (GitHub version, only if you link them)|WhatsApp and Telegram|Your messages and replies, exactly as the official apps send them. Sora uses an unofficial connection for WhatsApp: WhatsApp may limit accounts that use one.|
|Meeting recorder packages (GitHub version, only when you click Install)|GitHub (github.com) and Hugging Face (huggingface.co)|A standard download request for the model or engine you chose. Nothing about you is sent.|
|Meeting summaries in the cloud (GitHub version, off by default; only when you turn it on and click Summarize)|The AI service you choose and enter a key for (Claude, OpenAI, or another OpenAI-compatible server)|The transcript of that meeting and your key. Sora warns you before this. Your key is stored encrypted on your PC.|

These services receive your IP address as part of any internet request, and they handle it under their own privacy policies. Sora does not send them your name, your files, your events or any account details beyond what is listed.

## Sending and receiving files

When you use **Send** (LocalSend or Quick Share), files go directly from your computer to the device you choose on your local network. They do not pass through any Sora server. Sora looks for nearby devices on your network (and may use Bluetooth to wake nearby Android phones) only while you are scanning.

**Receiving with LocalSend:** while Sora runs, the LocalSend app on your own network can find it (Sora answers LocalSend's search with "Sora · " and your computer's name). A device that wants to send you files is shown under the notch with its name, the number of files and their size. Nothing is saved unless you click Accept; Deny, or no answer within a minute, refuses them. Accepted files go straight from that device into your Downloads folder and the Tray, with no server in between.

**Send to iPhone or Mac** shows a QR code and a separate 6-digit code. The phone opens a static page, connects straight to your computer over your local network and must enter the code before it sees any file name or can download anything. Files travel between your computer and that device, encrypted, with no Sora server in between. The link works once and closes after 30 minutes without use or when you press Disconnect. An upload from a phone needs your approval.

## Recording meetings (GitHub version)

The meeting recorder is off until you turn it on. It records only the app or audio you choose, shows a red dot in the notch while recording, and asks you to confirm before it records a browser or all PC audio. Tell the people in a meeting that you record it where the law or their expectations require it. Transcripts and summaries are made on your PC unless you turn on cloud summaries.

## Permissions

* **Windows:** location (only for "Use my location"), and the local network for Send and receiving files. The GitHub version also uses notification access when chat is on.
* **macOS:** Location (for "Use my location"), Local Network (for Send and receiving files) and Automation (to control Spotify or Music and open agent sessions in Terminal). macOS asks you before each one, and you can change them in System Settings > Privacy & Security.

## Children

Sora has no ads, no purchases and no content from other people. It is not directed at children and does not knowingly collect information from anyone.

## Changes

If this policy changes, the new version will be published with the "Last updated" date above.

## Contact

Questions about this policy: aqmilooooo@gmail.com
