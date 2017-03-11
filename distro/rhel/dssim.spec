Name:           dssim
Version:        1.3.1
Release:        1
Summary:        This tool computes (dis)similarity between two (or more) PNG images

Group:          Development/Tools
License:        AGPLv3
URL:            https://pornel.net/dssim
Source0:        https://github.com/pornel/dssim/archive/%{version}.tar.gz
BuildArch:      x86_64

BuildRequires:	make
BuildRequires:	libpng-devel

%description
This tool computes (dis)similarity between two (or more) PNG images using
algorithm approximating human vision. Comparison is done using the SSIM
algorithm (based on Rabah Mehdi's implementation) at multiple weighed
resolutions. The value returned is 1/SSIM-1, where 0 means identical image,
and >0 (unbounded) is amount of difference. Values are not directly comparable
with other tools.

%prep
%setup

%build
make

%install
rm -rf $RPM_BUILD_ROOT
mkdir -p $RPM_BUILD_ROOT%{_bindir}
install -m 0755 bin/dssim $RPM_BUILD_ROOT%{_bindir}/dssim

%clean
rm -rf $RPM_BUILD_ROOT

%files
%defattr(-,root,root,-)
%{_bindir}/dssim

%changelog
* Fri Jan 13 2016 Frank van Boven <frank@cenotaph.nl> - 1.3.0-1
- Updated the version 1.3.0

* Mon Apr 13 2015 Harry Danes <harry@danes.eu> - 0.9-1
- Initial release.
