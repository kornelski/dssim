#! /bin/bash

set -e 

ppa="ppa:lkwg82/dssim"


git-dch --release --git-author --commit --id-length=10 \
	&& dput $ppa $(find ../dssim*source.changes | sort | tail -n1)
	&& git push 
