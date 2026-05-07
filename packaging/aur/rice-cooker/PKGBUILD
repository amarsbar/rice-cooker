# Maintainer: ammarhossain <ammar at butterfly dot so>
pkgname=rice-cooker
pkgver=0.1.0
pkgrel=1
pkgdesc="a visual (toy) tool for ricing"
arch=('x86_64')
url="https://github.com/amarsbar/rice-cooker"
_srcdir="$pkgname-$pkgver"
license=('BSD-3-Clause')
depends=('electron41' 'git' 'hicolor-icon-theme' 'hyprland' 'polkit' 'qt6-5compat' 'qt6-multimedia' 'quickshell-git')
makedepends=('cargo' 'npm' 'rust')
optdepends=(
    'paru: preferred AUR helper for installing rice dependencies'
    'yay: alternative AUR helper for installing rice dependencies'
    'hyprpolkitagent: polkit authentication agent (any polkit agent works)'
    'ttf-material-symbols-variable: icon font for Whisker and Nandoroid'
    'ttf-outfit: intended UI font for Whisker'
)
source=(
    "$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz"
    'rice-cooker.sh'
    'rice-cooker.desktop'
)
sha256sums=('f94168d587b01d8eff3c46a763223e98558770f632b98c004c3cd84fc60ef8e7'
            '1527354310806f2ae55a2629d2954f02efa6b4063d85233ef3ecc50f53887b91'
            '70b83cade8c33465d876b517e7291e29d9ac95c7f6d81eafe9b8dde6b7a45907')

prepare() {
    cd "$_srcdir/backend"
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$_srcdir"
    npm ci
    npm run build

    cd backend
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release
}

check() {
    cd "$_srcdir/backend"
    cargo test --frozen --lib
}

package() {
    cd "$_srcdir"

    install -Dm755 "backend/target/release/rice-cooker-backend" \
        "$pkgdir/usr/bin/rice-cooker-backend"
    install -Dm644 "backend/catalog.toml" \
        "$pkgdir/usr/share/rice-cooker/catalog.toml"
    install -Dm644 "backend/polkit/so.butterfly.ricecooker.policy" \
        "$pkgdir/usr/share/polkit-1/actions/so.butterfly.ricecooker.policy"
    install -Dm644 "packaging/icons/rice-cooker.svg" \
        "$pkgdir/usr/share/icons/hicolor/scalable/apps/rice-cooker.svg"
    install -Dm644 "packaging/icons/rice-cooker.png" \
        "$pkgdir/usr/share/icons/hicolor/512x512/apps/rice-cooker.png"

    install -dm755 "$pkgdir/usr/share/rice-cooker"
    cp -a out package.json "$pkgdir/usr/share/rice-cooker/"
    install -Dm644 "packaging/icons/rice-cooker.png" \
        "$pkgdir/usr/share/rice-cooker/rice-cooker.png"

    install -Dm755 "$srcdir/rice-cooker.sh" "$pkgdir/usr/bin/rice-cooker"
    install -Dm644 "$srcdir/rice-cooker.desktop" \
        "$pkgdir/usr/share/applications/rice-cooker.desktop"
    install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
