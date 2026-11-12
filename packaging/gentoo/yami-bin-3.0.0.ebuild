# yami from the release's ready binaries (tools/dist.sh). From source: ./install.sh on Gentoo.
EAPI=8

DESCRIPTION="Lightweight pixel-art Wayland desktop for the niri compositor"
HOMEPAGE="https://github.com/Trendorin/yami"
SRC_URI="https://github.com/Trendorin/yami/releases/download/v${PV}/yami-${PV}-x86_64-linux.tar.gz"
S="${WORKDIR}/yami-${PV}-x86_64-linux"

LICENSE="MIT OFL-1.1"
SLOT="0"
KEYWORDS="~amd64"
IUSE="+files +view +play +shot greet"
RESTRICT="strip"
QA_PREBUILT="usr/bin/* usr/libexec/yami/*"

RDEPEND="
	gui-wm/niri
	sys-libs/pam
	sys-auth/polkit
	x11-libs/libxkbcommon
	play? ( media-video/mpv )
	shot? ( gui-apps/grim )
	greet? ( gui-libs/greetd )
"

src_install() {
	local p
	./stage.sh --prefix /usr --destdir "${D}" --managed --only yami-shell || die
	for p in files view play shot greet; do
		use "${p}" && { ./stage.sh --prefix /usr --destdir "${D}" --managed --only "yami-${p}" || die; }
	done
}
