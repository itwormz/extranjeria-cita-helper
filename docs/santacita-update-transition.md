# SantaCita release asset transition

Keep the existing GitHub release endpoint during the rename. Publish the
updater that recognizes both naming schemes in a release using the legacy
asset names. Installed versions with the old updater need that bridge before
they can discover SantaCita-only assets. If they skip the bridge, they will
need a manual installation unless the new release also carries legacy aliases.

| Target | SantaCita asset |
| --- | --- |
| macOS Apple Silicon | `SantaCita-VERSION-macOS-Apple-Silicon.dmg` |
| macOS Intel | `SantaCita-VERSION-macOS-Intel.dmg` |
| Windows x64 installer | `SantaCita-VERSION-windows-x64.msi` |
| Windows x64 portable | `SantaCita-VERSION-windows-x64-portable.zip` |
| Linux x86-64 | `SantaCita-VERSION-x86_64.AppImage` |
| Linux zsync companion | `SantaCita-VERSION-x86_64.AppImage.zsync` |

At the rename, update the packager, draft release asset checks and embedded
AppImage zsync pattern together. Generate SHA256SUMS and Sigstore proofs for
the actual published filenames, including any legacy aliases. When both
names exist the bridge updater selects the SantaCita asset and validates it
with the same checksum and provenance requirements.

The packaging switch should also verify in-place upgrades of the installed
app and reuse of profile data. Asset lookup is separate from the macOS bundle
identity, Windows installer upgrade identity and profile storage paths.
