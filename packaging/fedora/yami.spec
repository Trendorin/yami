# yami from the release's ready binaries (tools/dist.sh), one subpackage per program.
# rpmbuild -bb packaging/fedora/yami.spec, the archive in ~/rpmbuild/SOURCES
%global debug_package %{nil}
%global __brp_strip %{nil}

Name:           yami
Version:        2.7.0
Release:        1%{?dist}
Summary:        Lightweight pixel-art Wayland desktop for the niri compositor
License:        MIT
URL:            https://github.com/Trendorin/yami
Source0:        https://github.com/Trendorin/yami/releases/download/v%{version}/yami-%{version}-x86_64-linux.tar.gz
ExclusiveArch:  x86_64

%description
The yami shell for niri and its modules, each its own package.

%package shell
Summary:        Bar, launcher, dock, lock screen, notifications and widgets for niri
Requires:       niri pam polkit libxkbcommon
Recommends:     yami-files yami-view yami-play yami-shot udisks2 gvfs grim wl-clipboard cliphist playerctl brightnessctl
%description shell
The yami shell.

%package files
Summary:        File manager and the file dialog of every app
Requires:       libxkbcommon
Recommends:     ffmpegthumbnailer bsdtar udisks2 gvfs
%description files
yami-files.

%package view
Summary:        Image viewer with quick edits
Requires:       libxkbcommon
%description view
yami-view.

%package play
Summary:        Video and music player
Requires:       mpv-libs libxkbcommon
%description play
yami-play.

%package shot
Summary:        Screenshots, marks, text and QR
Requires:       grim libxkbcommon
Recommends:     tesseract zbar
%description shot
yami-shot.

%package greet
Summary:        Login screen for greetd
Requires:       greetd niri libxkbcommon
%description greet
yami-greet.

%prep
%setup -q -n yami-%{version}-x86_64-linux

%install
./stage.sh --prefix %{_prefix} --destdir %{buildroot} --managed

%files shell
%{_bindir}/yami
%{_bindir}/yami-shell
%{_bindir}/yami-agents
%{_datadir}/xdg-desktop-portal/portals/yami-agents.portal
%{_datadir}/dbus-1/services/org.freedesktop.impl.portal.desktop.yamiagents.service
%{_sysconfdir}/xdg/autostart/yami-shell.desktop
%{_datadir}/fonts/yami
%{_datadir}/yami
%{_datadir}/licenses/yami-shell

%files files
%{_bindir}/yami-files
%{_datadir}/applications/yami-files.desktop
%{_datadir}/xdg-desktop-portal/portals/yami.portal
%{_datadir}/dbus-1/services/org.freedesktop.impl.portal.desktop.yami.service
%{_datadir}/licenses/yami-files

%files view
%{_bindir}/yami-view
%{_datadir}/applications/yami-view.desktop
%{_datadir}/licenses/yami-view

%files play
%{_bindir}/yami-play
%{_datadir}/applications/yami-play.desktop
%{_datadir}/licenses/yami-play

%files shot
%{_bindir}/yami-shot
%{_datadir}/licenses/yami-shot

%files greet
%{_bindir}/yami-greet
%{_prefix}/libexec/yami/greet-session
%{_datadir}/yami-greet
%{_datadir}/licenses/yami-greet
