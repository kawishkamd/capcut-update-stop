# CapCut Update Blocker

Lock CapCut to the version you want and stop it from force-updating itself. If a
CapCut update has ever changed your editing setup or taken away something you relied
on mid-project, this freezes your version in place until *you* decide to move.

It's a small Windows app. It only touches CapCut's files on your own PC, and anything
it changes can be undone with one click. Not affiliated with CapCut or ByteDance.

## Get it

Download the latest `.exe` from the [Releases page](https://github.com/kawishkamd/capcut-update-stop/releases).
One file, no installer.

## Use it

Close CapCut, then right-click the exe and **Run as administrator** (it needs admin
rights to lock CapCut's files). You get three buttons:

- **Block Updates** - freezes your current version and shuts off the auto-updater.
- **Restore Original** - puts everything back exactly the way it was.
- **Download Installer** - fetches the official CapCut installer for a specific
  version, so you can install or roll back to the one you want.

Click Block and you're set. Want updates back one day? Hit Restore.

### Stronger lock (optional)

By default it sets CapCut's files to read-only, which stops most updates. If one
still slips through, tick **Stronger lock** before you block - it adds a Windows
permission rule the updater can't push past. Restore clears it either way. It's a
newer option, so do one Block-then-Restore first to be sure it behaves on your
machine. If a file ever feels stuck afterward, open an admin command prompt and run
`icacls "<file>" /reset`.

## Worth knowing

- **Close CapCut before running this** - it edits CapCut's config files.
- While you're blocked you won't get CapCut's newer features or fixes. That's the
  whole point, just don't forget you did it.
- Your antivirus might flag it. Anything that modifies another app's files tends to
  get flagged; the full source is in this repo if you want to look it over.
- Use at your own risk, and make sure you're fine with CapCut's Terms of Service.

## Support

Made by one person. If it saved you a headache, [a coffee](https://ko-fi.com/kayz) is
always appreciated.
