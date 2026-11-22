# Sasayaki Launcher (Dnet)

A Windows launcher that starts the Initial D series of arcade games through three loaders, **kitten / mice / segatools**:
kitten runs **D4/D5** (Lindbergh ELF), mice runs **D6/D7/D8** (RingEdge exe), and segatools runs **IDAC (InitialD The Arcade, ALLS) and IDZ (InitialD Arcade Stage Zero, Nu)**. kitten also runs **Wangan Midnight Maximum Tune 3 / 3DX / 3DX+** (Namco System N2 ELF).

- **Stack**: Rust + **windows-reactor** (Microsoft's official declarative WinUI 3 framework), giving a genuine native Windows 11 interface.
- **Native look**: Mica backdrop, Fluent controls, light/dark theme following the system; a 1180×720 client area, with an embedded icon and version resource (`build.rs` reads `APP_VERSION` from `main.rs`).
- **Game identification**: the user adds a game file by hand (`.exe` / `.elf` / **`ICF1`**). An exe/elf is looked up in the game database by its **CRC** (the algorithm reproduces the mice/kitten sources byte for byte). IDAC/IDZ have no CRC entries; their **ICF1 file** is decrypted to get the appid and version, which are then looked up.
- **Chinese, English and Japanese**: every user-facing text lives in `locales/zh-CN.yml` / `en.yml` / `ja.yml` and is looked up by key with rust-i18n's `t!` macro (see "Interface languages"). The language is switched on the Settings page (`config.ini [ui] lang`; when unset, it follows the system language).

> **Prerequisites**: the launcher is **framework-dependent**: the target machine needs the **Windows App SDK runtime** (2.x family). When it is missing or too old, a system error box is shown at start-up. The launcher checks its privileges and, **when not elevated, relaunches itself through `runas` (UAC)**: changing adapters with netsh and the awl/openvpn virtual adapters all need administrator rights.

## Usage

### 1. Placing the tools (important)

At start-up the launcher scans **the folder of its exe** (down to two levels of subfolders) for the loaders `kitten.exe` / `mice.exe` / `mice64.exe` / `inject.exe`. They can also be set by hand in `data\settings.json` as `kitten_path` / `mice_path` / `mice64_path` / `segatools_path` (these take precedence over detection). Recommended layout:

```
launcher folder\
├── launcher.exe
├── config.ini                  ← the launcher's own settings (created on first save)
├── data\                       ← game library / settings / caches (delete to reset)
└── tools\
    ├── kitten\                 ← kitten.exe (D4/D5) and all its dependencies
    ├── mice\                   ← mice.exe / mice64.exe (D6/D7/D8, chosen by the game's bitness)
    ├── segatools\              ← inject.exe and dependencies; idz\ and idac\ hold each game's hook dll etc.
    ├── asi-loader\             ← Ultimate-ASI-Loader (dinput8.dll, for the graphics patch)
    ├── shaderfix\              ← shaderfix plugin (deployed with winmm.dll as the game folder's scripts\)
    ├── dependency\             ← picodaemon.exe, cg.dll, cgGL.dll (copied into the game folder at launch; without the Cg dlls, the installed NVIDIA Cg Toolkit's are used)
    ├── awl\                    ← awl-tray.exe (in-store versus virtual LAN)
    └── openvpn\                ← openvpn.exe (+ wintun.dll, for the VPN)
```

The mice loader is chosen by the game's bitness: x64 games prefer mice64 (falling back to mice), x86 games prefer mice.

### 1.1 Launching a kitten game (automatic deployment + config generation, D4/D5 only)

Launching a game **does not connect or disconnect the VPN** (connect on the "VPN" page first when a remote server is needed).

Before the game starts, these steps run in order:

1. **Card reader preparation** (see 1.4): picodaemon is copied into the game folder from `tools\dependency\picodaemon.exe`; kitten games also get `cg.dll` / `cgGL.dll`.
2. The **top-level files of the kitten tool folder** are copied into the game folder (next to the game's `.elf`, overwriting files of the same name; `config.ini`, `*.log`, `sram.bin` and `eeprom.bin` are skipped, and a file whose content and modification time already match is left alone).
3. `<game folder>\kitten\config.ini` is **rebuilt from scratch**, with the launcher's network settings and the game's own settings (its settings page) as its only source (see the mapping table below).
4. `<game folder>\mice\config.ini` is **rebuilt** as well (mice format, the game-side IP, graphics settings ignored here too): the card reader's mice process uses the game folder as its home and reads network, force feedback and other settings from it.
5. Pre-launch checks (see 1.7); when they pass, the game starts as `kitten.exe -b <game file name>` (cwd = the game folder; only the file name is passed, because spaces in an absolute path get split and kitten reports `Failed to parse ELF file`; `CREATE_NO_WINDOW`, so no console window appears).

When **"Test mode"** is ticked on the game row, `-t` is appended to the command line. It applies to the game only, never to the ServerBox.

**kitten config.ini mapping** (plain `key = value` text without comments; keys that are not set are not written and empty sections are left out, so kitten's defaults apply; the whole file is rebuilt at every launch):

| Launcher setting | `[section] key` | Rule |
| --- | --- | --- |
| Graphics, with defaults turned off | `[display]` | Resolution → `dipsw=false` + `width` / `height`; borderless → `borderless`; render scale → `render_scale` (2/4/8); MSAA → `msaa_samples`; anisotropic → `anisotropic` (2/4/8/16). The whole section is left out while "Use default settings" is on |
| Server address (Settings page) | `[network] redirect` | The custom value; when unset, `sdgt.arcadehub.xyz` |
| Subnet | `[network] subnet` | The subnet CIDR as is (e.g. `192.168.100.0/24`) |
| Simulate network switch | `[network] simulateNetwork` | Unset counts as on; `false` is written only when explicitly turned off |
| Simulate network = off | `[network] bind_ip` / `game_ip` | ServerBox IP / host IP (the values entered on the Network page) |
| Log level | `[sysconf] log_level` | error→5 / warn→4 / info→3 / debug→1 / trace→0 |
| Region (game row) | `[sysconf] region` | JPN→0 / EXP→2 / CHN→2; CHN also adds `dipsw=0x3A` |
| Database game_id | `[sysconf] game_id` | Written when the game matched the database |
| Keychip | `[sysconf] keychip_serial` | As is |
| Skip card reader initialisation | `[sysconf] skipInitCard` | true/false |
| Coins | `[Coins]` | Free play → `freeplay`; coins for start / continue / card / car → `start` / `continues` / `card` / `car` |
| Input type | `[io3] mode` | xinput→1 / dinput→2; input ratio → `restrict_` (the default 128 when unset) |
| Input (xinput only) | `[xinput]` | `singleStickSteering` (the inverse of dual-stick mode), `linearSteering` (default true), `leftStickDeadzone` / `rightStickDeadzone` (32767 × percentage, default 20%); captured button names: `start` / `viewChg` / `up` / `down` / `left` / `right` / `shiftDn` / `shiftUp` / `coin` / `accel` / `brake` (not written when not captured or blank; kitten has no `card` key) |
| Input (dinput only) | `[dinput]` | `deviceName` / `pedalsName` / `shifterName`; button numbers `start` / `viewChg` / `shiftUp` / `shiftDn` / `coin` / `up` / `down` / `left` / `right` (D-pad pseudo codes ≥1000 are not written); throttle/brake **analog axis names** `accelAxis` / `brakeAxis`; `reverseAccelAxis` / `reverseBrakeAxis` |
| Force feedback (always written) | `[ffb]` | `enable` (default true), `constantForceStrength` / `damperStrength` / `rumbleStrength` / `rumbleDuration` (default 100), `baseDamperStrength` / `deadBand` (default 20) |

### 1.2 Launching a mice (RingEdge) game (automatic deployment + config generation, D6-D8)

1. **Card reader preparation** (as in 1.4; kitten's cg dlls are not needed).
2. The **top-level files of the mice tool folder** are copied into the game folder (except `config.ini` / `idas.ini` / `*.log` / `sram.bin` / `eeprom.bin`).
3. `<game folder>\mice\config.ini` and `mice\idas.ini` are generated (see the two tables below).
4. winmm.dll / scripts are deployed according to the game row's "Graphics patch" switch (see 1.6; both the game folder and the ServerBox folder).
5. `initialD.ini` of D4-D7 is rewritten byte by byte (see 1.5; D8 has no such file and is skipped silently).
6. When the checks pass: `mice.exe -p -b <game exe>` (cwd = the game folder, `CREATE_NO_WINDOW`; test mode adds `-t`).

**config.ini** (done the way kitten's is: plain key/value, only keys that are set are written; rebuilt at every launch; while no subnet is set, every network-derived key is left out):

| Launcher setting | `[section] key` | Rule |
| --- | --- | --- |
| (fixed) | `[mice] apply_patches` | Always `false` |
| Log level | `[mice] log_level` | error→2 / warn→3 / info→4 / debug→5 / trace→6; 4 (the info default) when unset |
| Keychip | `[sysconf] keychip_serial` | As is; not written when unset |
| Region (game row) | `[sysconf] region` | JPN→1 / EXP→4 / CHN→8; not written when unset |
| Coins, free play | `[sysconf] freeplay` | true/false |
| Subnet CIDR | `[sysconf] keychip_subnet` | As is |
| Derived from the subnet | `[network] ip_address` | Subnet base + **101 (game folder)** / + **102 (ServerBox folder)**; in room mode a member writes the seat IP it was given |
| Derived from the subnet | `[network] subnet_mask` | From the prefix (/24 → 255.255.255.0) |
| Derived from the subnet | `[network] gateway`, `primary_dns` | The last address of the subnet (broadcast − 1) |
| Server address | `[network] naominet_jp`, `aime_naominet_jp` | The custom value or the default `sdgt.arcadehub.xyz` |
| (fixed) | `[network] ib_naominet_jp` | `127.0.0.1` |
| Graphics, use defaults = true | — | The whole `[window]` section is left out |
| Graphics, borderless / resolution | `[window] borderless / dipsw / w / h` | A resolution also writes `dipsw = false` |
| Force feedback (always written) | `[ffb]` | As in the kitten table |
| Simulate network = off and the target is the ServerBox | `[ServerBox] bind_ip` | ServerBox IP (`172.10.1.102` in room mode) |

**idas.ini**:

| Launcher setting | `[section] key` | Rule |
| --- | --- | --- |
| (fixed) | `[ServerBox] offlineMode` | Always `false` |
| Simulate network switch | `[ServerBox] simulateNetwork` | Unset counts as on |
| Skip card reader initialisation | `[ServerBox] skipInitialization` | Same value, **always written** (unset → false) |
| Auto-insert card | `[CardReader] auto_insert` | **Always written** (unset → false) |
| Selected card | `[CardReader] cardfile` | The card chosen for this game; not written when none is chosen |
| Simulate network = off | `[ServerBox] bind_ip` / `game_ip` | ServerBox IP / host IP |
| Coins, start / continue / card / car | `[Coins] start / continues / card / car` | Numbers; keys left empty are skipped |
| Graphics, render scale | `[Graphics] render_scale` | 2x→2 / 4x→3 / 8x→4; **game side only**, never written on the ServerBox side |
| Input type / ratio | `[io3] mode / restrict_` | The strings xinput/dinput; the default 128 when no ratio is set |
| Input (xinput only) | `[xinput]` | `deviceIndex` (controller slot, default 0), `singleStickSteering` / `linearSteering` / left and right stick dead zones; captured button names: `start` / `viewChg` / `up` / `down` / `left` / `right` / `shiftDn` / `shiftUp` / `coin` / **`card`** / `accel` / `brake` |
| Input (dinput only) | `[dinput]` | `deviceName` / `pedalsName` / `shifterName`; button numbers `start` / `viewChg` / `shiftUp` / `shiftDn` / `coin` / **`card`** / `up` / `down` / `left` / `right` (D-pad pseudo codes are skipped); `accelAxis` / `brakeAxis`; `reverseAccelAxis` / `reverseBrakeAxis` |

> **Note**: config.ini and idas.ini are both **rebuilt from scratch at every launch** (the write is skipped when nothing changed, so the mtime stays), so do not edit these two files in the game folder by hand. Sections the launcher has no setting for, such as `[drivers]`, `[hooks]` and `[keys]`, are not generated and keep mice's defaults.

### 1.3 Launching a segatools game (IDAC / IDZ)

This applies only to games whose **loader is segatools** (SDGT = the IDAC seasons, SDDF = Zero, SDER = Zero CHN). Launch order:

1. **Generate segatools.ini**: the **top-level files of the segatools tool folder** are copied into **the folder of amdaemon.exe** (skipping `*.log` and the other game's hook dll), then the `*.dll` files of the `idz\` (IDZ) or `idac\` (IDAC) subfolder are copied there and a leftover hook dll of the other game is deleted. IDAC requires `idac\config_hook.json` (an error when missing), which is copied too. The `option\` and `appdata\` folders are created. IDAC also gets the spike file for its version (`idac\spike\indrun-<first three digits of the version>.txt`, e.g. `3.41.00 → indrun-341.txt`; when the tool folder has no such file, the `spike` key is not written).
2. **Start amdaemon**: `inject.exe -d -k <idzhook.dll | idachook.dll> amdaemon.exe -c <config list>` (cwd = the amdaemon folder). The config list depends on the game_id:
   - **IDAC (SDGT)**: 22 `config_*.json` files (common/ex/jp, laninstall, aime, seat_1~4/single, config_hook.json);
   - **IDZ (SDDF)**: 7 `configDHCP_Final_*.json` files (Common + JP/EX, each ST1/ST2);
   - **IDZ (SDER)**: 4 files (Common + ZH, each ST1/ST2).
3. **Start the game itself**: `inject.exe -d -k <hook dll> <game executable>` (cwd = the amdaemon folder, `CREATE_NO_WINDOW`):
   - **IDZ**: the executable is e.g. `InitialD0_DX11_Nu.exe`, with only `-m` appended;
   - **IDAC**: the executable is `GameProject-Win64-Shipping.exe`, with `-culture=<ja|en>` (region JPN→ja, otherwise en), `launch=Cabinet`, `ABSLOG=<Userdata>\GameProject.log`, `-Master`, `UserDir=<absolute Userdata path>`, `-NotInstalled` and `-UNATTENDED` appended (the Userdata folder is searched for up to 5 levels above the amdaemon folder).
4. When the game exits, amdaemon is killed, with `taskkill` as a fallback.

**segatools.ini mapping** (plain key/value, unset keys not written, empty sections left out; rebuilt at every launch):

| Launcher setting | `[section] key` | Rule |
| --- | --- | --- |
| (fixed) | `[vfs] amfs / option / appdata`, `[netenv] enable=1`, `[system] enable=1`, `[led15070] enable=1` | Always written |
| (fixed) | `[aime] enable=1` | Always written |
| Selected card | `[aime] aimePath` | `DEVICE\<20-digit card number>.txt`; when no card is chosen IDZ falls back to `DEVICE\aime.txt` and IDAC writes nothing |
| (fixed, IDZ only) | `[aime] felicaGen=0`, `aimeGen=1` | Always written |
| Server address | `[dns] default` | The custom value or the default domain; `router` is always `10.0.0.254` |
| Host IP (IDZ only) | `[netenv] addrSuffix` | The last byte of the host IP (the seat number inside a room); not written when not configured |
| Keychip | `[keychip] id` | As is |
| Database game_id | `[keychip] gameId` | Written when matched |
| Subnet | `[keychip] subnet` | The subnet base (without prefix); `172.10.1.0`, the room subnet, in room mode |
| Region (game row) | IDAC → `[keychip] region`; IDZ → `[ds] region` | JPN→1 / EXP→4 / CHN→8 |
| (fixed, IDZ only) | `[pcbid] serialNo`, `[ds] serialNO` | Generated at random at every launch (`ACAE01A########` / `AAVE-01A########`) |
| Coins, free play | `[system] freeplay` | 1/0 |
| Host checkbox | IDZ → `[gpio] dipsw1`; IDAC → `[system] dipsw1` | 1 = host |
| Seat (IDAC only) | `[system] dipsw2~5` | SINGLE→(0,1,0,0), A1→(0,0,0,0), A2→(0,1,0,0), B1→(0,0,1,0), B2→(0,1,1,0) |
| (fixed, IDAC only) | `[indrun] enable=1` (+ optional `spike`) | Always written |
| (fixed, IDZ only) | `[gfx] enable=1`, `windowed=1` | The graphics hook is always on and always windowed; with "Use default settings" turned off, `width` / `height` are added (1920×1080 when no resolution is chosen or it fails to parse) and `framed` (borderless → 0, otherwise 1). IDAC has no `[gfx]` section |
| Input type / ratio | IDAC → `[io4]`; IDZ → `[io3]`: `mode` / `restrict` | xinput/dinput; the default 128 when no ratio is set |
| Input (xinput only) | `[xinput]` | `autoNeutral=1`, `singleStickSteering` / `linearSteering` / left and right dead zones; captured button names `start` / `viewChg` / `up` / `down` / `left` / `right` / `shiftDn` / `shiftUp` / `coin` / `accel` / `brake`; the **card scan button** goes to `[aime] xiScan` + `xiDeviceIndex` (controller slot), not to `[xinput] card` |
| Input (dinput only) | `[dinput]` | The three device names, `accelAxis` / `brakeAxis`, button numbers (D-pad pseudo codes skipped) `start` / `viewChg` / `up` / `down` / `left` / `right` / `shiftDn` / `shiftUp` / `gear1~6`, `reverseAccelAxis` / `reverseBrakeAxis`; the force feedback strengths are in this section too: `constantForceStrength` / `damperStrength` / `rumbleStrength` / `rumbleDuration` / `baseDamperFraction` / `deadband` |
| Force feedback switch | `[ffb] enable` | Unset counts as on |

### 1.4 Card reader (picodaemon)

kitten and mice games get their card reader prepared **before launch** (segatools games emulate aime themselves and have no card reader process):

1. **Provisioning**: when the game folder has no `picodaemon.exe` (or has one whose CRC is not a known one), a copy is taken from `tools\dependency\picodaemon.exe`; if that is missing, the launch stops with an error. kitten games also get `cg.dll` / `cgGL.dll`: both from `tools\dependency\` when it has both, otherwise both from the installed NVIDIA Cg Toolkit (the folder its installer records in `CG_BIN_PATH`).
2. **`mice.exe` and `mice.dll`** of the mice tool folder are copied into the game folder (overwriting files of the same name, skipping identical ones).
3. `<game folder>\mice\idas.ini` is written: `[CardReader] auto_insert` (the "Auto-insert card" switch on the System tab of the game's settings page) and `cardfile` (the card chosen for this game), `[ServerBox] game_ip` (this machine's game IP).
   - **kitten games only** also get the input backend and the card button: `[io3] mode` (from the input type on the game's Input tab) and `[xinput] card` (the captured card button's name, not written when not captured). The kitten game process does not handle card insertion; the reader-side mice reads it.
   - For **mice games** this file is then rebuilt as a whole by the config generation of 1.2 (the `card` button is already in the game-side idas.ini, so the reader side does not write it again).
4. The reader starts as `<game folder>\mice.exe -p -b picodaemon.exe` (cwd = the game folder; **its console window stays visible**); the launcher waits 1 second before starting the game, and closes the reader when the game exits.

### 1.5 Rewriting initialD.ini for a private server (D4-D7)

Before D4-D7 start, the launcher decides from the database `game_id` (SBML/SBQZ/SBTS → idas4/5, SBUU/SBVW/SBWV → 6, SBYD/SBYY → 7) whether to rewrite `initialD.ini` in the **game folder** (found case-insensitively). The rewrite works **line by line on the raw bytes**: the original Shift-JIS encoding, the Japanese comments, CRLF and alignment spaces are kept and only the value after `=` is replaced; the file is written through a temporary file and an atomic replace, and left untouched when nothing changes:

1. Every key containing `SERVER_ADDR` (MATCH / USERDB / EVENT / SCREENSHOT / CARDB, …) → the "Server address" from the Settings page (the default `sdgt.arcadehub.xyz` when unset)
2. `MATCH_SERVER_PORT_TCP_*` → `10003`
3. `MATCH_SERVER_PORT_UDP_SEND_*` and `TAG_MATCH_SERVER_PORT_UDP_SEND_*` → `12353`
4. `MATCH_SERVER_PORT_UDP_RECV_*` and `TAG_MATCH_SERVER_PORT_UDP_RECV_*` → `12354`
5. `USERDB_SERVER_PORT_TCP_*` → `10000`
6. `EVENT_SERVER_PORT_TCP_*` → `10000`
7. `ADV_NEWS_SERVER_URL_*` → `http://<server address>:8086/idas<N>/news/news50.txt` (N = version 4~7)

Every index (`_0~_3` on D5, `_0~_15` on D6/D7) gets the same value. Lines commented out with `;` / `#`, `TAG_MATCH_SERVER_PORT_TCP_*`, `STORE_*` and other keys not listed are left alone. Games other than D4-D7, or a missing file, are skipped silently without blocking the launch. The rewrite is idempotent.

### 1.6 Graphics patch (winmm.dll + shaderfix)

mice games and kitten ServerBoxes support a **graphics patch** (a switch in the game row's expanded card, shown only on mice game rows, on by default):

- On: `dinput8.dll` from `tools\asi-loader\` (Ultimate-ASI-Loader, searched recursively in subfolders) is copied as **`winmm.dll`** into the target folder, then every file of `tools\shaderfix\` is copied flat into the target folder's **`scripts\`**.
- Off: an existing `winmm.dll` in the target folder is deleted (scripts is left in place).
- Scope: mice games = the game folder + its ServerBox folder; kitten games = the ServerBox folder only (and with the switch on, a missing asi-loader component is an error asking to update it on the Settings page first).
- The **graphics settings** (resolution, borderless, …) still apply to the game only: configs generated for the ServerBox always drop the graphics keys (kitten `[display]`, mice `[window]` / `render_scale`, the whole segatools graphics set).

### 1.7 Pre-launch checks and process lifetime

After "Launch" is clicked (the launch panel overlay shows progress; on failure a "Launch failed" dialog can jump to the relevant settings page):

1. **Common checks**: the region is set (the game row drop-down, **required**; the old global `[system] region` is used only as a fallback when generating configs and does not satisfy this check) → the keychip is filled in → a subnet is configured (segatools accepts the room network instead).
2. **kitten/mice extras**: dinput mode does not support an H-pattern shifter (refused, jumping to the Input tab of the game's settings page); a car card is selected; a ServerBox is configured.
3. **segatools extras**: a car card is selected; amdaemon.exe and the game executable are configured.
4. Start-up order: card reader (1 s) → [segatools: amdaemon] → game. If any step fails, the launch stops and the processes already started are closed too.
5. **ServerBox static IP**: when the network is not simulated and not in room mode, and the adapter currently has fewer than 2 IPv4 addresses, `netsh` adds the ServerBox IP as a static address on the chosen adapter (DHCP and static coexisting). It is removed automatically when the game exits or is closed by hand.
6. **Clean-up on exit**: the game process is polled (1 s); after it exits, the card reader and amdaemon child processes are killed, `taskkill /f /im` cleans up `amdaemon.exe` and the ServerBox process names (IDZ games also `ServerBoxD8_Nu_x64.exe` / `ServerBoxD8_Nu_x64_CHN.exe`), the static IP added above is removed, and the play count and last-played time are stored in the library.

### 1.8 ServerBox (started by hand)

The ServerBox is **no longer started together with the game**. It is started on demand with "Launch" on the game row (or the room page), and ends on "Close" or when the game exits:

- **mice**: deploy first (copy the tool files + generate `mice\config.ini` in the ServerBox folder with `ip_address` = subnet base + 102; `idas.ini` is generated too, without graphics keys), then `mice.exe -b <ServerBox exe>` (cwd = the ServerBox folder).
- **kitten**: deploy the graphics patch to the ServerBox folder according to the switch, run the kitten deployment once more (tool files + `kitten\config.ini` + `mice\config.ini`, without graphics keys), then `kitten.exe -b <ServerBox file>`.
- **segatools**: the ServerBox is not supported yet (an explicit error).
- The ServerBox file must not be in the game's folder, and its CRC must equal the `sb_crc` of that build in the database.

### 2. Adding games

**"+ Add game"** in the top bar opens the system file dialog with `.exe`, `.elf`, `ICF1`, and the extensionless `main` file used by Maximum Tune 3 / 3DX / 3DX+ visible by default.

- **exe / elf / main**: in the background the CRC is computed by file type (PE → mice, ELF32 → kitten) and looked up in the database. On a match the game is added with the product name + build version (the cover is the **built-in cover image** named by the database's `image`, e.g. `logo_D8.png`); otherwise a naming dialog opens (the loader follows the file type: ELF → kitten, EXE → mice).
- **ICF1** (the encrypted version information file of IDAC/IDZ): decrypted with AES-128-CBC and a fixed key, checked with two CRC32s, and the appid and `major.minor.build` version (e.g. `SDGT 3.41.00`) are looked up as `game_id + version`. An id/version not in the database is an error.

### 3. Interface layout (Windows 11 Settings style)

**Top bar**: title + DNET website link (`https://dnet.arcadehub.xyz`) + "+ Add game".
**Navigation on the left**: Games / In-store versus / Network / VPN / Settings (the card selection page and a game's settings page are not in the navigation; they are opened from a game row).

**Games page**: a list of long cards in the Windows 11 Settings style. The games are listed under a heading per series (Initial D, then Wangan Midnight Maximum Tune; the family decides), each by the release date of its database product, with versions out the same day in the database's order (JPN, EXP, CHN); a game the database does not know comes last in its series, in the order added. Only the display is sorted: `library.json` keeps the order added, since row actions refer to games by their place in it. Each row has "cover thumbnail (the user's file if set, otherwise the built-in cover of its database product (`image`, with `<name>_big.png` on the launch panel), otherwise the "work in progress" image `src/assets/cover_wip.png`, which a game the database does not know shows; the database's cover and platform are preferred to those saved in `library.json` when the game was added, so a newer database shows on games added before) + game name + subtitle (version · game_id · ServerBox configured) + platform badge (Lindbergh / RingEdge / ALLS / Nu) + an arrow at the end". Clicking the row header expands its configuration card in place:

| Row | Description |
| --- | --- |
| **Launch game** | The "Launch" button; kitten/mice also have a "Test mode" checkbox; while running it becomes "Close" |
| **Graphics patch** (mice only) | A switch, see 1.6 |
| **Card selection** | Shows the current card (segatools shows `<card number>.txt`, the others `.png`); "Open" opens the card page |
| **Region** | Drop-down JPN / EXP / CHN (**required**, per game) |
| **ServerBox** | Add / launch / change / remove; "Close" while running; shown only for games whose matched build has an `sb_crc` |
| **amdaemon.exe / game executable** (segatools only) | Stores the file paths; add / change / remove |
| **Game settings** | "Open" opens the game's settings page (see below) |
| **Remove game** | Deletes the entry after confirmation (including its cover cache and its settings file) |

**Game settings page** ("Game settings" → "Open" on a game's row): the settings of that one game, on three tabs, Graphics, System and Input. Each game keeps them in a file of its own, `games\<id>.ini` next to `config.ini` (`<id>` is the game's id in `library.json`), with the sections and keys `config.ini` used for them before: `[graphics]`, `[coin]`, `[input]`, the game's keys of `[system]`, and `[wmmt3]` for a WMMT3 game. An Initial D game's tabs are described below; a WMMT3 game's in "Game families". A launch reads the launched game's file (a change still waiting to be saved is saved first).

**Moving the old settings**: earlier versions kept these settings in `config.ini`, shared by every game. At start, each game in the library without a file of its own gets a file with them, so it launches as it did before; then they are taken out of `config.ini` (if a file cannot be written, nothing is taken out and an error is shown). With an empty library they stay in `config.ini` until the first game is added. A game added later starts from the defaults.

**Graphics tab**: default settings (a switch; when on, the options below are disabled and ignored), borderless mode, resolution (1360x768 / 1920x1080 / 2560x1440 / 3840x2160), render scale (on + 2x/4x/8x), MSAA (on + 2x/4x/8x/16x), anisotropic filtering (on + 2x/4x/8x/16x). Everything is saved to the `[graphics]` section of the game's settings file. The graphics settings apply to the **game only** (IDZ maps them into segatools.ini `[gfx]`; the ServerBox always ignores them).

**Network page**: adapter drop-down (connected adapters only, with "Refresh adapters") → that adapter's IPv4 subnet drop-down (CIDRs without duplicates) → **simulate network switch** (on by default) → **host IP** field → **ServerBox IP** field (editable only with the network not simulated). Choosing an adapter pre-fills host IP = its first IPv4 address and ServerBox IP = its second (or the first + 1 when there is none). When the chosen adapter is the awl virtual adapter `awl0`, both IP fields are locked to the room's values. Everything is saved to `config.ini [network]`.

**VPN page**: import an `.ovpn` config, private key passphrase, connect/disconnect, status light. See the "VPN" section.

**System tab**: log level (info by default / error / warn / debug / trace), skip card reader initialisation, **auto-insert card**, **seat settings** (a "Host" checkbox + a SINGLE/A1/A2/B1/B2 drop-down, used for the IDAC/IDZ dipsw and to derive the room seat). The keys of the `[system]` section of the game's settings file: `loglevel` / `skip_reader_init` / `auto_insert_card` / `seat` / `master` (switches write 1/0, empty means unset). The keychip is the launcher's (Settings page); `config.ini [system]` keeps `keychip` and `region` (only as a fallback for old configs; the UI moved to the game row).

**Coin settings** (an expandable card on the System tab): free play and the coins for start / continue / buying a car card / buying a car (0~99), saved to the `[coin]` section (`freeplay` / `start_coin` / `continue_coin` / `card_coin` / `car_coin`).

**Input tab** ("Scan devices" and "Device diagnostics" at the top right):

- **Basics**: the test / service / coin keys are fixed to F1/F2/F3 (shown for information); input mode (xinput / dinput); input signal limit (a 0~255 slider, default 128, resettable).
- **Force feedback card** (the config is always written): switch (on by default) + constant force / damper / rumble strength / rumble duration (default 100) + base damper / centre dead band (default 20) sliders.
- **xinput**: controller (slot 0~3), dual-stick mode (off by default, affects `singleStickSteering`), linear steering (on by default), left and right stick dead zones (0~100%, default 20%), **12 button mapping slots** (shift up / shift down / the four directions / start / view / coin / card / accelerator / brake). After "Capture", the button pressed on the controller within 10 seconds is recorded by name (defaults RB/LB/D-pad/A/B/X/Y/RT/LT; duplicates are highlighted in red as conflicts).
- **dinput**: wheel / pedal / shifter device drop-downs (told apart by device path when several interfaces share a name); throttle/brake **axis capture** (+ an "invert axis" checkbox); shifter type (sequential / H); button capture (shift up/down, the four directions, start / view / coin / card, gears 1~6, reverse; the D-pad is recorded as pseudo codes ≥1000, which are skipped when writing the ini); duplicates are highlighted in red as conflicts.

**Settings page**: launcher version (`APP_VERSION`), language (中文 / English / 日本語, each in its own language), server address (empty means the default `sdgt.arcadehub.xyz`; used for kitten redirect / mice naominet / segatools dns / initialD.ini), keychip (required by the Initial D games and the rooms; it names this PC to the server and in the rooms, so it is the launcher's rather than a game's), room nickname (at most 24 characters, default `player1234`), automatic update check switch, a "Check now" button, and the **component update table** (see "Automatic updates").

**Status bar at the bottom**: four status lights, server (probing TCP 443/80 of the address on the Settings page every 15 seconds) / VPN / room / update.

## 4. Card selection

Open "Card selection" from the expanded game row. The game determines the card format.

**Initial D PNG cards (kitten / mice)**

- **Card display**: the built-in base image + the card image from the game folder **centred over the base image at its size** (moved down 4 px, and left 5 px for ID4), shown scaled to about 0.8 and laid out in as many columns as fit. Compositing is warmed up in batches and cached; on failure the card slot shows the reason and a status bar dialog sums them up.
- **The base image follows the game** (embedded resources, **no longer replaceable at run time**): D4/D4kai → the ID4 base, D6 → the ID6 base, D8 → the ID8 base, everything else → the ID5 base.
- **Where cards come from**: `.png` files in the game folder (≤3 levels of subfolders, at most 512 folders) that contain an **`INID` PNG chunk** (2048 bytes of card data, the micetools card format), usually in `disk0\mice\`.
- **Blank car card**: a "Blank car card" tile is always first in the list; clicking it makes a random `InidCrdXXXXX.png` name the current card (a blank card is selected automatically when there are no cards at all).
- **Clicking a card**: stores it in the game's `card` field (`library.json`; the old global `config.ini [card] cardfile` is still the fallback). The current card has an accent-coloured outline and an "In use" label.

**Aime cards (segatools, IDAC/IDZ)**

- Cards are `.txt` files in the amdaemon folder's **`DEVICE\`** (content = a 20-digit access code). Scanning renames any file name to `<card number>.txt`, removes duplicates, and puts newer cards first by modification time.
- "+ New car card" is first in the list: it writes a random 20-digit card number starting with `5` and refreshes. The card number is drawn over the card in groups of four.

**Maximum Tune 3 / 3DX / 3DX+ cards (KittenTools)**

- The card list shows 207-byte `.bin` files in the game's `cards` folder or the location set by `[wmmt3] card_dir`. When available, the matching `<filename>.bin.png` supplies the preview.
- Selecting a card saves its filename to the game's library entry and sets `[wmmt3] card_file` on launch. With no launcher selection, KittenTools uses its configured card filename or its default `card.bin`.
- The new-card tile selects an unused filename. KittenTools writes the data when the player buys a new card in the game.
- The game's System tab has an automatic insertion toggle that sets `[wmmt3] card_auto_insert`. Games configured with `card_reader_port` use their external reader.
- KittenTools keeps three more files beside each card: `<filename>.bin.face.png` (what is printed on it), `<filename>.bin.stock.txt` (the file name of its blank card image) and `<filename>.bin.png` (the print on that image, the preview above). The launcher only reads the preview; a card copied elsewhere needs all four files.
- The game's System tab also has a "Card printing" section: the folder of blank card images (`[wmmt3] print_template`; a card from the dispenser gets one of its images at random and keeps it), the print font file (`print_font_file`) and its name (`print_font_name`, filled in with the family name read from the font file whenever the file is chosen or typed). The folder and the font file are typed or chosen with "Browse…" (a folder dialog, or a file dialog for fonts). Each game has its own folder, such as one per game of 3, 3DX and 3DX+.

A back button sits at the top of the page.

## 5. In-store versus (room virtual LAN)

A peer-to-peer virtual LAN run on a managed **Anywherelan (awl)** plus a room directory service, linking up to 4 seats as in a store.

**Network plan** (fixed): room subnet `172.10.1.0/24`, virtual adapter `awl0`; the host's game IP is `172.10.1.101` (seat A1) and its ServerBox IP `172.10.1.102` (only D4-D8 need the second IP; IDAC/IDZ hosts use a single IP); members' seats follow the last byte of the IP they are given: 103=A2, 104=B1, 105=B2. Inside a room the network is **forced to not simulated**, and the subnet / host IP / ServerBox IP all switch to the room's values (the game configs are rewritten automatically; the original settings are not affected after leaving).

**Directory service** (`https://dnet.arcadehub.xyz/roomapi`, overridable with `config.ini [room] directory`): `POST /rooms` (create a room, bound to the keychip), `GET /rooms?game_id=`, `POST /rooms/{id}/join` (checks the password, returns the host's peer_id/IP and this machine's seat IP), `GET /rooms/mine?keychip=`, `GET /rooms/{id}/members`, `POST /rooms/{id}/leave`, `DELETE /rooms/{id}?keychip=` (close the room), `POST /rooms/{id}/heartbeat`. A room holds 4.

**Creating a room**: entering the page requires a keychip (otherwise it is blocked and jumps to the Settings page). "Create…" picks a game (the database's product list) + room name + password (empty = a public room). Flow: check the `awl-tray.exe` version (`-v` must contain `v0.19.0-mp2`) → create the awl identity the first time (`data\awl\config_awl.json`, kept and reused) → write the awl config (node name sasayaki, HTTP API `127.0.0.1:8639`, TUN off / on as needed, DNS off) → register firewall rules (`Sasayaki Awl` allow + ICMP) → start `awl-tray.exe` (`AWL_DATA_DIR=data\awl`, waiting up to 60 s for the local API) → create the room in the directory → add the ServerBox unicast IP to `awl0`. When leaving the room or closing the program, awl is shut down gracefully (API quit, killed after 5 s, with a Job Object as the last resort).

**Joining a room**: the lobby's public list (name / game / players, "Join" / "Full") or "Join…" with an ID + password. Flow: join in the directory to get this machine's seat IP → write the awl config with that IP and start it → invite the host (alias=host, with the host's game / ServerBox IPs + domain) → wait up to 30 s for the host to accept → correct the IP with `update_settings`.

**Member reconciliation**:

- The **host** ticks every second (every 15 s when not on the room page): fetches the directory's member list (at most every 60 s, or at once when an unknown connected peer appears); invites members not invited yet at their seat IP (members also invite the host back, forming a full mesh); a member who dropped out is removed with leave after **90 s if never connected, or 10 s if once connected**; renews the room with a heartbeat every 300 s; publishes the member list to the sync service.
- A **member** ticks every second (every 15 s when not on the room page): leaves the room automatically when it receives the host's close frame (on any page); on the room page it also polls the awl peers every 2 s to watch the host (declared dead after 10 s, leaving automatically with a notice); receives the member list incrementally from the sync table (leaving automatically when it finds itself removed); invites the other members in the list (mesh); reports changes of its playing state at once (30 s keep-alive).

**Member state sync**: the host listens on TCP `172.10.1.101:27510` with a JSON-lines protocol: upstream `hello` / `status` (peer_id, nickname, playing); downstream a versioned member list (`SyncTable`: each member's peer / IP / nickname / playing state + the host's nickname / state) and `{"type":"close"}`. A state without updates for 75 s counts as idle; a dropped connection reconnects after 1 s.

**Room page (in a room)**: room ID / password (copyable) / game / this machine's game address / ServerBox address (shown for D4-D8); the member table (seat / member / address / status light / playing / latency / direct or relayed); "Launch game", "Launch ServerBox" (host only, when configured), "Close room / Leave room". **Closing the room** first notifies the members (close frame), then deletes the room in the directory with the keychip (this fixed "the room does not disappear"), and finally removes every awl peer and quits awl.

**Recovering after a disconnect**: if the launcher is killed, the `--room-watchdog` child process it forked in advance waits for the parent to exit and closes the room (3 retries). After leaving the room page normally, entering it again "returns to the room" automatically: with a keychip it first asks `rooms/mine` to restore the host role, otherwise it joins the remembered `last_room` automatically (the password is stored DPAPI-encrypted in `config.ini [room]`).

## 6. VPN

The "VPN" page manages an **OpenVPN** connection (for remote play, unrelated to rooms):

- `openvpn.exe` goes in `tools\openvpn\` (or an absolute path in `config.ini [vpn] openvpn=`). With `wintun.dll` in the same folder the wintun driver is preferred, automatically falling back to tap-windows6 on failure (and the other way round without wintun; only driver errors are retried).
- Import an `.ovpn` (its path and the private key passphrase are stored in the `[vpn]` section, the passphrase DPAPI-encrypted). Connecting writes a temporary password file (`--askpass` / `--auth-user-pass`), and adds `--ping 10 --ping-restart 60` when the config has no keepalive directive. Seeing `initialization sequence completed` in the log within 25 s counts as success; on failure or timeout the error lines are taken from the log and shown (wintun-type errors add "make sure wintun.dll is in the same folder").
- A configured VPN **connects automatically at start-up**. A dropped connection is retried 3 times (the count resets after a connection stable for ≥300 s); after that it is marked failed with a prompt to reconnect by hand. A guard thread polling every 2 s watches the process, and a Job Object makes sure the process tree exits with the launcher.

## 7. Automatic updates of components and the launcher

The launcher has 7 built-in update targets and takes updates straight from the latest Release of each component's repository, with no manifest to maintain:

| Target | Asset match (case-insensitive) | Source |
| --- | --- | --- |
| **Sasayaki Launcher** (self-update) | `launcher*.zip` | GitCode `project-sasayaki/launcher` |
| kittentools | `kitten*.zip` | GitHub / GitCode `project-sasayaki/kittentools` |
| micetools | `mice*.zip` | GitHub / GitCode `project-sasayaki/mice-inid-release` |
| segatools | `segatools*.zip` | GitHub / GitCode `project-sasayaki/segatools-inid-release` |
| Ultimate-ASI-Loader | `ultimate-asi-loader*nopdb*` (excluding x64) | GitHub `ThirteenAG/Ultimate-ASI-Loader` / GitCode mirror |
| shaderfix-idas | `shaderfix.zip` | Gitea `gitea.tendokyu.moe/puniru/shaderfix-idas` |
| awl | `awl*.zip` | GitHub / GitCode `project-sasayaki/awl` |

- **Checking**: once at start-up, then every hour (controlled by the "Check for updates automatically" switch); "Check now" bypasses the cache. For targets with two sources, the interface language picks one (Chinese → GitCode, any other language → GitHub). Results are cached for 24 hours in `data\update_cache.json` (with a `schema` version, so a change of the parsing rules invalidates it automatically).
- **Picking the "latest" Release**: the single-object endpoint `/releases/latest` is preferred; an array endpoint is reordered by "`release_status == "latest"` → latest `created_at` → last in the array" and the first is taken (GitCode returns releases oldest first, so taking the first one directly would give the oldest). When the latest one has no matching asset, the next older one in the same order is used. Pinning `.../releases/tag/v1.4.2` is provided by the built-in sources; the code supports pinning a tag as well.
- **Versions**: always the Release tag without `v` (e.g. `1.4.2`), compared numerically part by part (`1.10` > `1.9`). The local baseline is `last_<component>_version` in `data\settings.json` (written back after a successful install); **the launcher's baseline is the compile-time `APP_VERSION` constant**.
- **Download and verification**: WinHTTP streams to disk (a `.part` temporary file) with live progress (download / extract / install, cancellable). When a GitHub asset carries a `digest` (`sha256:...`), it is verified after the download.
- **Installing (tools components)**: extract to `tools\.__update_<component>\` → strip a single top-level folder → rename the old folder as a backup → move the new one in → delete the backup; rolled back automatically on failure. The loader paths are scanned again after installing.
- **Installing (launcher self-update)**: after extracting, the files are **merged over** the launcher folder (skipping `launcher.exe` / `data\` / `.old` at the top level, so user data is untouched) → the old exe is renamed and moved into a temporary folder → the new exe is swapped in atomically → the new version is started with `--self-restart <old pid>` and the old one exits (the new process waits for the old one to exit; `AllowSetForegroundWindow` keeps it in the foreground). Leftovers of an update (`launcher.exe.old`, `.__launcher_backup`, `.__update_launcher`) are cleaned up at start-up.
- **Rules**: no updates while a game is running; when the launcher itself has an update, it must be updated before any other component ("Update all" follows this too). When a new version is found, the status bar shows "Update available" and a "Component updates available" dialog opens (with a way to the Settings page).

## Game families

The games are grouped into families that are launched the same way (`src/games/`):

| Family | Games | Loader | Card | ServerBox |
| --- | --- | --- | --- | --- |
| `IdasLindbergh` | Initial D Arcade Stage 4 / 5 | kitten | INID PNG, picodaemon reader | yes |
| `IdasRingEdge` | Initial D Arcade Stage 6 / 7 / 8 | mice | INID PNG, picodaemon reader | yes |
| `Idac` | Initial D The Arcade | segatools (idachook) | Aime | no |
| `Idz` | Initial D Arcade Stage Zero | segatools (idzhook) | Aime | no |
| `Wmmt3` | Wangan Midnight Maximum Tune 3 / 3DX / 3DX+ | kitten | (in kitten's config) | no |

`games::family_of` picks the family from the record's loader and game id (kitten games with the game id `WM3` / `W3X` / `W3P` are `Wmmt3`). Each family implements `GameFamily`:

- `ui_features`: the game row controls and selected-card extension; `card_kind`: the card page format and artwork (unknown IDs retain the existing ID5 fallback);
- `check`: the settings this family needs (the Initial D families all need the region, keychip and network);
- `prepare`: write the configs and copy the tools into the game folder;
- `start`: start the helpers (card reader, amdaemon) and the game, and say what to end when it exits;
- `launch_serverbox`: start the ServerBox, for the families that have one.

`games::launch_game` runs these in order (sections 1.1–1.7 describe what each family does), and adds the ServerBox's static IP between `prepare` and `start` for families with a ServerBox. The interface gathers a `LaunchInput` from the settings in effect and shows the result; a `LaunchProblem` names the page where the user can fix a missing setting. The game row and card page take their family-specific choices from these methods. The ServerBox row remains conditional on the matched build having a ServerBox CRC.

**Wangan Midnight Maximum Tune 3 / 3DX / 3DX+** (`games/wmmt3/`): the launch copies kitten's files next to the game's `main`, writes the launcher's settings into `kitten\config.ini`, and starts `kitten.exe -b main`. KittenTools keeps every setting of these games in its `[wmmt3]` section (apart from `[sysconf] log_level`). The launcher offers them on the game's settings page: the Graphics tab (screen), the System tab (log level, automatic card insertion, card printing, free play, linked play, sound, load times, dongle) and the Input tab (the controls and the coin, card, test and service switches on the device of the input mode in use, the keyboard keys, force feedback); a setting is shown only while it has an effect (the window size without the cabinet's resolution, the second dongle on 3DX+, …), and its value is kept while hidden; `games/wmmt3/settings.rs` lists them with KittenTools' names and defaults, and must follow KittenTools' `wmmt3Config.def` when that changes. The values set there are kept in the `[wmmt3]` section of the game's own settings file, and written into the game's `kitten\config.ini` at launch together with `[sysconf] log_level` (System tab); a setting never changed in the launcher is not written, and the rest of the file stays as it is, to be edited by hand. These games need no keychip, region or subnet, and use the built-in card reader in KittenTools by default. The test button is a key. They are not offered in rooms: their linked play needs the cabinets in one /24 network that carries multicast, which the rooms do not provide. Their covers are `logo_wm3`, `logo_wm3dx` and `logo_wm3dxplus` (each with a `_big` version), shared by the regional versions.

**Adding a game family**: write `src/games/<family>.rs` implementing `GameFamily`, and return it from `family_of`. Shared steps (card reader, graphics patch, ServerBox, `initialD.ini`) are in their own modules; the loader modules in `src/loaders/` only render config files.

## Interface languages

The texts are compiled into the binary from `locales/<locale>.yml` by rust-i18n (`rust_i18n::i18n!` in `main.rs`; `build.rs` makes an edited locale file rebuild the crate). Code refers to a text by key: `t!("page_games.launch")`, or with values `t!("common.failed_to_copy", src = path.display(), error = e)`, whose `%{src}` / `%{error}` placeholders the locale files fill in. `en.yml` is the base; a key missing from another locale would show its English text.

- Keys are grouped by where they are used (`page_games`, `launch`, `update`, …); texts several places share are in `common`, the navigation entries in `nav`, and the errors of the HTTP client in `http`.
- Tables whose entries carry a label (the input bindings, the log levels, the seats) hold the label's key and translate it where it is shown.
- `config.ini [ui] lang` stores `zh-CN` / `en-US` / `ja-JP`. With no (or an unknown) value the Windows locale decides: `zh-*` → Chinese, `ja-*` → Japanese, anything else → English.

`cargo test` checks the catalog: every locale has exactly the keys of `en.yml` with the same placeholders, every `t!` call in the sources names an existing key and passes exactly the placeholders of its text, and every text is used somewhere.

**Adding or changing a text**: add the key to all three locale files (in the same section), then use it. **Adding a language**: add `locales/<locale>.yml`, a `Lang` variant with its locale, native name and `config.ini` value in `i18n.rs`, and its case in `Lang::from_locale_name`.

## Data storage (portable, no installation)

`data\` next to the exe:

- `library.json`: the game library. Each record has `path` / `name` / `crc` / `version` / `arch` / `loader` (kitten|mice|segatools) / `platform` / `image` (the user's cover file) / `cover` (built-in cover name) / `serverbox` / `amdaemon` / `game_exe` / `game_id` / `gfx_patch` / `test_mode` / **`card` (card chosen per game)** / **`region` (region per game)** / `play_count` / `last_played` / `added_at` (older files with missing fields are read compatibly).
- `settings.json`: loader paths (kitten/mice/mice64/segatools), `auto_check_updates`, and each component's `last_*_version`.
- `images\`: cover cache; `awl\`: awl identity and config (`config_awl.json`); `update_cache.json`: update cache.

`config.ini` next to the exe (`ini.rs` changes single keys and keeps the rest; password-type keys are DPAPI-encrypted): `[network]` (adapter / ip / simulate / server / host_ip / serverbox_ip), `[system]` (keychip / region), `[card]` (the global card, the fallback for the per-game card), `[vpn]` (config / passphrase / openvpn), `[room]` (directory / awl / last_room / last_password / nickname), `[ui]` (lang). Delete it to reset.

`games\<id>.ini` next to it, one per game (see "Game settings page"): `[system]` (loglevel / skip_reader_init / auto_insert_card / seat / master), `[graphics]`, `[coin]`, `[input]` (type / ratio / devices / buttons / force feedback / xinput slot, about 40 keys), `[wmmt3]` (WMMT3 games: their settings under KittenTools' names). Delete one to reset that game.

## Game database (games_db.json)

A **hand-maintained** nested JSON file (embedded at build time). Top-level structure:

```jsonc
{
  "version": 1,
  "games": [
    {
      "name": "InitialD Arcade Stage 8 JPN",
      "game_id": "SBZZ",
      "platform": "RingEdge",
      "loader": "mice",                          // kitten = D4/D5, mice = D6-D8, segatools = IDAC/IDZ
      "image": "logo_D8.png",                    // built-in cover name (must exist in src/assets/)
      "support_game": [
        { "crc": "0x1fd6136a", "version": "2.01", "sb_crc": "0x8667432a" },
        { "version": "3.41.00" }                 // segatools entries may have no crc (matched only through ICF1)
      ]
    }
  ],
  "card_reader": {                               // picodaemon, for the CRC check
    "name": "picodaemon", "platform": "RingEdge", "loader": "mice",
    "support_game": [
      { "crc": "0x037ff93c", "version": "2014" },
      { "crc": "0xd2939f28", "version": "2007" }
    ]
  }
}
```

- Products: D4/D4kai/D5/D5EXP (kitten), D6AA JPN/EXP/CHN, D7AAX JPN/CHN, D8 JPN/EXP (mice), **IDAC Season 1~6 (SDGT, version-only entries)**, **IDZ / IDZ Ver.2 (SDDF), IDZ CHN (SDER)** (segatools), **WMMT3 JPN/EXP (WM3), 3DX JPN/EXP (W3X), 3DX+ JPN/EXP/CHN (W3P)** (kitten, System N2; versions are the builds' Namco revision letters, e.g. `B02`). A product with `"in_store_battles": false` is not offered in rooms. `release` is the date the game came out in Japan (`YYYY-MM-DD`; its export versions share it), from Japanese Wikipedia (頭文字D ARCADE STAGE, 稼働開始; 湾岸ミッドナイト MAXIMUM TUNEシリーズ, 発売日); the Games page sorts by it.
- Adding a game matches a build of a product by "file type → loader + CRC". `sb_crc` is the CRC of that build's ServerBox (missing / null / an empty string all mean no ServerBox, and the ServerBox row is not shown). The ICF1 path matches `game_id + version (2 or 3 parts, compared for equality)`.
- `card_reader` is used for the card reader's CRC check (when picodaemon is provisioned into a game folder, an existing file is checked against it).

### CRC algorithms (byte for byte the same as the C sources)

| Loader | Algorithm | Range |
| --- | --- | --- |
| mice | Standard CRC-32 (crc32fast) | The raw bytes of the PE `.text` section |
| kitten | Standard CRC-32 (equivalent to JAMCRC with an XOR) | 0x4000 bytes of the ELF image from `base+10`; gaps between PT_LOAD segments are hashed as zeros |

## Building

```bash
cargo build --release   # produces target/release/launcher.exe + the bootstrap dll (framework-dependent, no bundled runtime)
cargo test              # 200+ unit tests (CRC reproduction / database lookup / ICF decryption and parsing /
                        # every config rendering mapping / card reader and dependency deployment / graphics patch /
                        # ini persistence / update parsing and installation / room sync protocol, …)
```

- `build.rs`: framework-dependent mode (`windows_reactor_setup::as_framework_dependent()`), and writes the exe icon and `VERSIONINFO` from `APP_VERSION` in `src/main.rs` (both the numeric and the string copies agree).
- Release conventions (for the automatic updates): the version is the Release tag (the version is the tag without `v`); the launcher asset is named `launcher*.zip`, and **must not contain `data\`** (the self-update merge skips it, but keep the package clean); tools component assets are `kitten.zip` / `mice.zip` / `segatools.zip` / `shaderfix.zip`, etc.
- Dependency source: `windows-reactor` / `windows-reactor-setup` (official git dependencies of [microsoft/windows-rs](https://github.com/microsoft/windows-rs)).

## Folder structure

```
├── src/
│   ├── main.rs           # Entry point: UAC relaunch, --self-restart / --room-watchdog sub-modes, update leftover clean-up, runtime bootstrap
│   ├── reactor_app.rs    # WinUI 3 main component (navigation / top bar / status bar / message dispatch) + the pages and dialogs under reactor_app/
│   ├── games/            # The game families and how each is launched (see "Game families")
│   │   ├── mod.rs        # GameFamily trait, LaunchInput, launch_game / launch_serverbox, family_of
│   │   ├── idas_lindbergh.rs, idas_ringedge.rs, idac.rs, idz.rs, wmmt3/   # one family each (wmmt3/settings.rs: its settings table)
│   │   ├── idas.rs, alls.rs                # steps the IDAS / segatools families share
│   │   ├── card_reader.rs  # picodaemon card reader: provisioning, idas.ini, start (with unit tests)
│   │   ├── gfx_patch.rs    # winmm.dll + shaderfix graphics patch (with unit tests)
│   │   ├── serverbox.rs    # ServerBox folder / record / graphics patch, netsh static IP (with unit tests)
│   │   └── initiald.rs     # Byte-level private-server rewrite of initialD.ini (D4-D7, with unit tests)
│   ├── loaders/          # The loaders and their config files
│   │   ├── mod.rs        # Loader scan and resolution (settings path or next to the launcher)
│   │   ├── kitten.rs     # kitten: config.ini rendering + tool copying (with unit tests)
│   │   ├── mice.rs       # mice: config.ini / idas.ini rendering (game and ServerBox targets) + CIDR derivation (with unit tests)
│   │   ├── segatools.rs  # segatools: segatools.ini rendering (IDAC and IDZ forms), amdaemon / game command lines, spike
│   │   └── render_util.rs  # Shared config rendering helpers (Sections / tool file copying / dead zone conversion, with unit tests)
│   ├── file_util.rs      # File copying helpers for deployment (with unit tests)
│   ├── icf.rs            # ICF1 decryption (AES-128-CBC) + appid / version parsing (with unit tests)
│   ├── db.rs             # Game database lookup (CRC index + game_id/version index, with unit tests)
│   ├── detect.rs         # PE/ELF detection + CRC reproduction + picodaemon probing (with unit tests)
│   ├── cards.rs          # INID PNG card scanning + Aime card creation / scanning (with unit tests)
│   ├── storage.rs        # Data model and JSON persistence + reading/writing config.ini and each game's games\<id>.ini (with unit tests)
│   ├── ini.rs            # Generic single-key INI read/write that keeps comments (with unit tests)
│   ├── netinfo.rs        # Adapter and IPv4 subnet enumeration (GetAdaptersAddresses, with unit tests)
│   ├── rawinput.rs       # HID device enumeration and button / axis / XInput capture
│   ├── update.rs         # Component and launcher self-update (Release parsing / download / verification / installation, with unit tests)
│   ├── awl.rs            # Anywherelan lifetime (create / join / invite / IP plan / firewall, with unit tests)
│   ├── roomdir.rs        # Room directory REST client (with unit tests)
│   ├── room_sync.rs      # Room member state sync TCP protocol (host and member sides, with unit tests)
│   ├── vpn.rs            # OpenVPN management (driver fallback / log diagnosis / Job Object, with unit tests)
│   ├── server.rs         # Server probing thread
│   ├── i18n.rs           # Interface languages: the choice, config.ini value, system default; catalog consistency tests
│   ├── filedialog.rs     # Native file dialog
│   ├── win_util.rs       # Small helpers: wide strings / CREATE_NO_WINDOW / DPAPI, …
│   └── assets/           # Embedded resources: card base images (id4/id5/id6/id8/aime), game cover logos, launcher.ico
├── locales/              # Interface texts per language (zh-CN.yml / en.yml / ja.yml, embedded at build time)
└── games_db.json         # Game database (hand-maintained, embedded at build time)
```
