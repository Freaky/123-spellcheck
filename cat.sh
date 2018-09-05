#!/bin/sh

echo >&2 -- "$@"
exec cat >&2

