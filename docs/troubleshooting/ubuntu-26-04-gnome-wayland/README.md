# ZER0 on Ubuntu 26.04 GNOME Wayland

Adapted from the upstream project's notes (tested there on Ubuntu 26.04.1 LTS,
GNOME Wayland). ZER0 has not been run on this setup by its maintainer: the
steps are the same mechanisms, with ZER0's names and paths.

The fix is to use `ydotool` for typing and the native keyboard implementation
for shortcuts.

_Note: run the commands one by one in a terminal._

## 1. Check Wayland and uinput

```bash
echo "$XDG_SESSION_TYPE"
id
ls -l /dev/uinput
grep -R 'uinput' /etc/udev/rules.d /usr/lib/udev/rules.d 2>/dev/null
```

You should be using `wayland`, be in the `input` group, and have `/dev/uinput`
owned by `root:input` with mode `0660`. Ubuntu provides the required udev rule
in `80-uinput.rules`.

If you are not in `input`:

```bash
sudo usermod -aG input "$USER"
```

Log out and back in — **and check the user manager**: with autologin it
survives the logout and starts every new session with the old group list, so
logging out and back in is not enough:

```bash
# Was the manager started before the group change?
ps -o pid,lstart -p "$(systemctl show --property=MainPID --value user@$(id -u))"

# If it predates the change (or the group is still missing after logging in again):
sudo systemctl restart user@$(id -u)
```

A reboot also works. Restart ZER0 afterwards.

## 2. Install and test ydotool

```bash
sudo apt install ydotool
systemctl --user start ydotool
systemctl --user status ydotool --no-pager
ydotool type "HELLO FROM YDOTOOL"
```

The service should show `active (running)`, and the test should type into the
focused application.

## 3. Configure ZER0

Set the typing tool to `ydotool` and the keyboard implementation to the native
one (its stored value is `handy_keys`, the name of the library it uses). Quit
ZER0 first, then edit its settings:

```bash
sed -i 's/"typing_tool": "auto"/"typing_tool": "ydotool"/' ~/.local/share/com.nairodorian.zer0/settings_store.json
sed -i 's/"keyboard_implementation": "tauri"/"keyboard_implementation": "handy_keys"/' ~/.local/share/com.nairodorian.zer0/settings_store.json
```

Start ZER0 again:

```bash
zer0 --start-hidden &
```

Then press your transcription shortcut and test a dictation. If the shortcut
does nothing, the log (Debug page, or the log file in the app data folder)
says why the keyboard backend could not start.

## 4. Hide the overlay

On GNOME the overlay is an ordinary window that can take the focus, and then
nothing is pasted. Set the overlay to **None** on the Overlay page.

## 5. Install wl-clipboard

ZER0 uses `wl-copy` on Wayland when it is installed:

```bash
sudo apt install wl-clipboard
```

## 6. Non-QWERTY keyboard layouts

`ydotool` sends physical keys, so the built-in paste methods fail on layouts
such as bépo or Dvorak: Ctrl+V presses the QWERTY `V` key, and Shift+Insert
breaks while a modifier of your shortcut is still held.

Use an external script instead: it presses Ctrl and the key that types `v` on
your layout, `KEY_U` (22) on bépo or `KEY_DOT` (52) on Dvorak (codes in
`/usr/include/linux/input-event-codes.h`).

Create `~/.local/bin/zer0-paste`, here for bépo:

```sh
#!/bin/sh
printf '%s' "$1" | wl-copy
sleep 0.1
ydotool key 29:1 22:1 22:0 29:0
```

```bash
chmod +x ~/.local/bin/zer0-paste
```

Then set **Paste Method** to **External Script** with this path.
