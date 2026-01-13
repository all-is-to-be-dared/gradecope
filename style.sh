#!/bin/bash

export RST="\x1b[0m"

export RED="\x1b[31m"
export GRN="\x1b[32m"
export YEL="\x1b[33m"
export BLU="\x1b[34m"
export PRP="\x1b[35m"
export CYA="\x1b[36m"

export BLD="\x1b[1m"
export ITA="\x1b[3m"
export UND="\x1b[4m"

export FERR="${RED}ERROR${RST}"
export FWRN="${YEL}WARNING${RST}"
export FINF="${GRN}${BLD}INFO${RST}"

export STUSR="${UND}"
export STGRP="${UND}${CYA}"

export STVER="${BLD}${YEL}"
export STPTH="${ITA}${PRP}"
export STPKG="${ITA}${BLU}"

export STGOP="${BLD}${GRN}"

