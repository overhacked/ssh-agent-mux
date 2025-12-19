#!/bin/sh

set -e

echo "Preparing release..."
# update the version
version=$(git cliff --bumped-version)
msg="# bumped by release.sh"
sed -E -i '' -e "s/^version = .* $msg\$/version = \"${version#v}\" $msg/" Cargo.toml
cargo build
# update the changelog
git cliff --bump --output CHANGELOG.md -- v0.0.0..
git add -A
git commit -m "chore(release): prepare for $version"
git show
# generate a changelog for the tag message
changelog=$(git cliff --bump --unreleased --strip all | sed -e '/^#/d' -e '/^$/d')
# create a signed tag
git tag -f -s -a "$version" -m "Release $version" -m "$changelog"
#git tag -v "$version"
echo "Done!"
echo "Now push the commit (git push) and the tag (git push --tags)."
