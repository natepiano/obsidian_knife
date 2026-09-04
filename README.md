# obsidian_knife - aka "ok"

[![CI](https://github.com/pianonate/obsidian_knife/actions/workflows/ci.yml/badge.svg)](https://github.com/pianonate/obsidian_knife/actions/workflows/ci.yml)

CLI utility to maintain [Obsidian](https://obsidian.md) repositories by automating backlinks, image cleanup, and date maintenance.
# usage
the binary for obsidian knife is "ok" - clever, eh?
```bash
ok <config_file.md>
```
The config file must be a markdown file with yaml frontmatter - an example can be found in the examples folder.## configuration

```yaml
# required
obsidian_path: ~/Documents/obsidian            # path to obsidian vault - tilde is allowed or you can specify full path
output_folder: obsidian_knife                  # where to place output file (relative to obsidian_path)

# optional
apply_changes: false                           # true to apply changes, false for dry-run
back_populate_file_filter: [[some note]]       # optionally process this specific file for back population
do_not_back_populate:                          # text patterns to skip during back population
  - bill
  - will
file_limit: 10                                 # limit files processed - if this parameter is not specified it will process all files
ignore_folders:                                # folders to skip during processing
  - templates
operational_timezone: America/New_York         # see note below
```
It's important that the yaml is placed between lines with only --- in them to mark the beginning and ending of the
frontmatter in the markdown file. Then you can place the configuration file in your output_folder (which by default is
ignored when scanning the repo).

This way you can see both the configuration and the output as markdown files within your obsidian repo.  It's not
required that you place the configuration file there but it can be convenient.

# preview changes
Review proposed changes in "obsidian knife output.md" before enabling apply_changes.

# features
- dry-run support with detailed change preview
- back-populate wikilinks for existing content - useful for when you create a topic and would like existing text to have links added to match the topic
- detect and report invalid wikilinks
- detect and report yaml frontmatter errors
- clean up images:
  - remove duplicates
  - remove broken image references
  - remove zero-byte images
  - remove non-rendering formats (tiff)
- stamp date_modified on the files it changes

## date handling
Currently, obsidian_knife (hereafter referred to an "ok") is hard coded for how i use dates in obsidian - as yaml
properties in the markdown front matter like so:
```
---
date_created: "[[2024-10-22]]"
date_modified: "[[2024-11-06]]"
---
```
the front matter is the record of when a note was written. ok never reads a file's filesystem timestamps and never
reconciles these properties against them. a vault cloned with git, restored from a backup, or copied between machines
carries the transfer time on every file, so those timestamps say nothing about the note.

date_created belongs to whatever creates the note - the obsidian linter plugin stamps it on creation. ok never writes
one. if you disagree with a date, edit the markdown and ok will leave your edit alone. a file with no date_created is
listed in the frontmatter issues report so you know to run the linter on it.

ok does stamp date_modified on the files it changes itself - back population, link rewrites, image reference cleanup -
because the linter only runs when obsidian saves a file and would never see those edits.

a file whose front matter is missing or unparseable is reported and skipped. ok never invents a front matter block and
never replaces one it could not read.

also at some point providing the name of the frontmatter property should become configurable as well

### operational_timezone
we can set an operational time zone (defaults to: America/New_York time zone). For more information on naming,
see [IANA time zones](https://data.iana.org/time-zones/tzdb-2021a/zone1970.tab)

the operational time zone decides which calendar day a date_modified stamp lands on. the operating system hands ok a
UTC instant and the front matter wants a wikilink date, so ok converts to the operational timezone before formatting it.

As an example, 23:00 on the East Coast is 04:00 of the next day UTC. A file changed at 23:00 on 2024-01-15 in New York
is already 2024-01-16 in UTC. With operational_timezone: America/New_York the stamp reads [[2024-01-15]] - the date on
the wall clock where you are - and it stays that way no matter which timezone you happen to run ok in.

the obsidian linter plugin can do most of what i'm doing here with dates but it doesn't have the notion of the operational timezone.
it does allow you to conver to UTC but if you don't want to operate in UTC then this doesn't work

## useful troubleshooting info
ok will output a list of any files that have invalid frontmatter. Those files are never modified: back population,
link rewrites, and image cleanup all skip a file whose frontmatter block exists but can't be parsed, so the block is
left for you to repair.

ok will output any invalid wikilinks so your repo doesn't get messed up

## back populate behavior
Any existing wikilinks found in your markdown files will be back populated. Useful when you create a topic and
want to get every instance in your repo that could target that new topic to have a link to it.

for example, if you create a new topic for [[OLED Displays]] and you already have a bunch of notes that refer to
OLED Displays, then back populate will add links to the existing text. It's a useful search and replace.

If you have an alias in the wikilink such as [[OLED Displays|OLED]], then OLED will also become a target for
replacing with [[OLED Displays|OLED]] so it will still render as OLED in obsidian (but now with a link)

If you have linked text but haven't created the note then no note will be created but other text that matches
that link will also get the link attached.

Every .md page in your repo will also get added as a wikilink to back populate in case you haven't already linked them up.

If you have the property "aliases" in your markdown frontmatter, they will also be created as links that can
be back-populated.  For example, this is the frontmatter for a page named sugar.md

```
---
aliases:
  - brown sugar
  - white sugar
  - powdered sugar
date_created: "[[2024-08-27]]"
date_modified: "[[2024-10-26]]"
tags:
  - ingredient
---
```

if your text has the phrase "brown sugar" in it, then ok will replace it with [[sugar|brown sugar]] - useful!

because of the potential for edge cases i haven't thought of - you can run ok in dry run mode with apply_changes
set to false so you can verify the changes before they happen.

### ambiguous wikilinks
if two different pages have the same alias - for example, if you have pages for people and they have the same
first name which you use as an alias, then back population can find two different target pages for the same text.

because of this, ok will not replace these with wikilinks but instead will show them to you so you can take
action and change them to whichever target you wish.

ok will protect you!
## images
images are hashed to determine whether there are file duplicates. if there are, then one will be chosen to be kept
and the rest will be deleted and any references to the deleted images will be updated to point at the one that is kept.

this may or may not work for you and it is not currently configurable so you'll either need to fork the code and
remove this functionality or wait for me to make it a configurable capability.

Any images that are not referenced by files will be deleted - very destructive!

Any images that can't render (TIFF, Zero-Byte length files) will be deleted - very destructive!

# configuration details

## obsidian_path
Required. Path to your Obsidian vault. Supports shell expansion using `~` for home directory.

## output_folder
Required. Location for the "obsidian knife output.md" file. Path is relative to obsidian_path.

This output folder will be automatically added to ignore_folders. As such it's a convenient place for you to
store your configuration.md file if you wish.

## apply_changes
Optional. Default: false
- false: dry-run mode, only shows proposed changes
- true: applies all changes shown in output file

if you have this configuration.md (or whatever you name it) in obsidian, then the apply_changes will output as a
radio button you can click to enable.

After ok does an update with apply_changes: true, it will set this property back to false
so you don't accidentally apply changes when you may not want to - especially when making sure that things work.

## file_limit
Optional. Limits the number of files processed. Useful for testing changes on a subset of files.

For example, you can set apply_changes: false and then limit the number of files processed so you can assess if ok is
doing the right thing.  once your happy with the results, you can either remove this property or set it
to a very large number.

## back_populate_file_filter
Optional. Process only a specific file for back population. Value can be in wikilink format (`[[note]]`) or
plain text (`note.md`). Useful for debugging.
## do_not_back_populate
Optional. List of text patterns to exclude from back population. Useful for:
- Common phrases that should not become wikilinks
- Text that renders the same as a note title or alias but shouldn't be linked

Each pattern is matched case-insensitively as a complete word.

In my repo i have a file for a friend named Will. The file is his full name but Will is an alias.  I don't want
the word Will to be turned into [[Will A Friend|Will]] everywhere so will one of my do_not_back_populate entries
in my config.

do_not_back_populate is special in that you can also add it as a yaml property on any of your pages to prevent
substituting wikilinks just on that page. The page property accepts a single value (`do_not_back_populate: style`)
or a list, the same as `aliases`.
## ignore_folders
Optional. List of folders to skip during processing. Paths are relative to obsidian_path. The output_folder
from the configuration file is automatically added to this list.

Files and folders whose names start with a dot (`.obsidian`, `.trash`, `.git`, `.DS_Store`) are always skipped:
Obsidian never indexes them, so a link into one can't resolve in the app and a report about it would describe
notes the vault can't see.
# cache
ok creates a `.ok` folder in your vault to store image hashes. This cache improves performance when
checking for duplicate images across multiple runs. Especially in larger repos.

# shell commands
one of the obsidian plugins is called Shell commands - with this you can compile Obsidian Knife to a binary and
place it anywhere you wish, then configure a Shell command for it that you can then invoke from within obsidian
via the command palette. On my system once compiled and configured, I just type "command-P" (for the command palette)
then "ok" for the shell command (which is how i configured it's Alias in Shell commands). "command-P ok" - simple.
