# AUR package

`PKGBUILD` for [`shard-launcher-bin`](https://aur.archlinux.org/packages/shard-launcher-bin): it repackages
the `.deb` from a GitHub release.

Local install without AUR:

```bash
cd packaging/aur
makepkg -si
```

Publishing a new version to AUR:

```bash
# bump pkgver, then
updpkgsums
makepkg --printsrcinfo > .SRCINFO
# copy PKGBUILD and .SRCINFO into the AUR git repo and push
```
