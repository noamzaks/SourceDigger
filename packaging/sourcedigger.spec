Name:           sourcedigger
Version:        0.1.0
Release:        1%{?dist}
Summary:        Browse when the symbols of C projects were added, removed or changed
License:        AGPL-3.0-only
URL:            https://github.com/wazzaps/sourcedigger
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.gz
Source2:        %{name}-%{version}-web.tar.gz

BuildRequires:  cargo-rpm-macros
BuildRequires:  gcc
BuildRequires:  pkgconfig(openssl)
BuildRequires:  pkgconfig(zlib)
BuildRequires:  systemd-rpm-macros

%description
SourceDigger indexes the functions, defines and variables of C projects in every
tagged version of their git repository, and serves a web interface and a JSON API
that show when each symbol was added, removed or changed, with its code.

%prep
%autosetup
tar --extract --file=%{SOURCE1}
tar --extract --file=%{SOURCE2}
find vendor -type f -name '*.rs' -exec chmod -x {} +
%cargo_prep -v vendor

%build
%cargo_build
%cargo_vendor_manifest

%install
install -Dpm 0755 target/rpm/sourcedigger-server %{buildroot}%{_bindir}/sourcedigger-server
install -Dpm 0644 packaging/sourcedigger.service %{buildroot}%{_unitdir}/sourcedigger.service
mkdir -p %{buildroot}%{_datadir}/%{name}
cp -a web/dist %{buildroot}%{_datadir}/%{name}/web

%post
%systemd_post sourcedigger.service

%preun
%systemd_preun sourcedigger.service

%postun
%systemd_postun_with_restart sourcedigger.service

%files
%license LICENSE cargo-vendor.txt
%doc README.md
%{_bindir}/sourcedigger-server
%{_unitdir}/sourcedigger.service
%{_datadir}/%{name}/

%changelog
* Thu Nov 12 2026 Noam Zaks - 0.1.0-1
- First package
