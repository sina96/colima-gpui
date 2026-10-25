Build on macOS with Xcode Command Line Tools and the pinned Rust toolchain:

```sh
just app  # dist/Colima GPUI.app, release build for this Mac's architecture
just dmg  # dist/Colima GPUI.dmg, containing the app and an Applications shortcut
```

Colima, Lima and Docker remain external CLI dependencies. The fonts are embedded
in the binary. No third-party bundler or extra Rust crate is needed.

The bundle is ad-hoc signed and verified. Developer ID signing and notarization
are outside this target's scope. Builds target macOS 11 or newer; Colima's VM
backend may require a newer OS. Build on each architecture to produce its app;
this target does not produce a universal binary.

The app icon comes from `assets/brand/ColimaGPUI.icon` (an Icon Composer document). After
changing it, rebuild and commit the generated files:

```sh
just icons  # packaging/AppIcon.icns, packaging/Assets.car (Liquid Glass, macOS 26) and assets/brand/icon.png
```

This needs Xcode 26 or later. `Assets.car` also needs, once, the Xcode license accepted and
Xcode's system components installed (open Xcode and let it install them); without it, macOS
uses `AppIcon.icns`. Building the
app itself (`just app`) never needs Xcode.

For the Finder launch check, open `dist` in Finder and double-click `Colima GPUI.app`.
Use only the disposable `gpui-test` profile for start/stop checks. The shared
runner resolves Homebrew/Nix binaries and supplies the child's PATH; Finder need
not inherit a terminal's PATH.
