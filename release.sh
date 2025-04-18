#!/bin/sh

set -e

if [ -z "$1" ]; then
	echo "Please provide a tag."
	echo "Usage: ./release.sh v[X.Y.Z]"
	exit 2
fi

echo "Preparing $1..."
# update the version
msg="# bumped by release.sh"
sed -E -i '' -e "s/^version = .* $msg\$/version = \"${1#v}\" $msg/" Cargo.toml
cargo build
# update the changelog
git cliff --tag "$1" --output CHANGELOG.md v0.0.0..
git add -A
git commit -m "chore(release): prepare for $1"
git show
# generate a changelog for the tag message
changelog=$(git cliff --tag "$1" --unreleased --strip all | sed -e '/^#/d' -e '/^$/d')
# create a signed tag
git tag -f -s -a "$1" -m "Release $1" -m "$changelog"
#git tag -v "$1"
echo "Done!"
echo "Now push the commit (git push) and the tag (git push --tags)."
