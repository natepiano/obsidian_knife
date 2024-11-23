# obsidian_knife - aka "ok"

CLI utility to maintain [Obsidian](https://obsidian.md) repositories by automating backlinks, image cleanup, and date maintenance.

## usage
the binary for obsidian knife is "ok" - clever, eh?
```bash
ok <config_file.md>
```

The config file must be a markdown file with yaml frontmatter - an example can be found in the examples folder. 


## configuration

```yaml
# required
obsidian_path: ~/Documents/obsidian            # path to obsidian vault
output_folder: obsidian_knife                  # where to place output file (relative to obsidian_path)

# optional
apply_changes: false                           # true to apply changes, false for dry-run
back_populate_file_count: 1                    # limit files processed during back population 
back_populate_file_filter: [[some note]]       # optionally process this specific file for back population
do_not_back_populate:                          # text patterns to skip during back population
  - bill
  - will
ignore_folders:                                # folders to skip during processing
  - templates
operational_timezone: America/New_York
```
It's important that the yaml is placed between lines with only --- in them to mark the beginning and ending of the frontmatter in the markdown file. Then you can place the configuration file in your output_folder (which by default is ignored when scanning the repo). 

This way you can see both the configuration and the output as markdown files within your obsidian repo.  It's not required that you place the configuration file there but it can be convenient. 

## preview changes

Review proposed changes in "obsidian knife output.md" before enabling apply_changes.

## features
- dry-run support with detailed change preview
- back-populate wikilinks for existing content - useful for when you create a topic and would like existing text to have links added to match the topic
- detect and report invalid wikilinks
- detect and report yaml frontmatter errors
- clean up images:
  - remove duplicates
  - remove broken image references
  - remove zero-byte images
  - convert non-rendering formats (tiff)
- manage frontmatter dates and file creation times

### date handling
Currently, obsidian_knife (hereafter referred to an "ok") is hard coded for how i use dates in obsidian - as yaml properties in the markdown front matter like so:
```
---
date_created: "[[2024-10-22]]"
date_modified: "[[2024-11-06]]"
---
```
if the date_created doesn't match the file date created, ok will update date_created to match the file's actual create date

if the file modify date is different from the property in the file, then the property will be updated

if you want to change the file create date to something else you can add a property called "date_create_fix" to the front matter with the date that you'd like the file to have.  ok will change the file create date, update the date_created property and remove the date_create_fix property after.

at some point, i may make this a configurable feature - for now it's default behavior

#### operational_timezone
we can set an operational time zone (defaults to: America/New_York time zone). For more information on naming, see [IANA time zones](https://data.iana.org/time-zones/tzdb-2021a/zone1970.tab)

the operational time zone bridges the gap between the wikilink dates of date_created and date_modified and the UTC date from the operating systme.

This way, mismatches between the OS and the frontmatter will always treat the frontmatter as if it's in the operational timezone so when you happen to run obsidian knife in some other time zone, it won't change all of the create and modified dates to line up with the timezone you happen to be in at that moment in time. It will keep aligning them all to whether the OS date on the east coast matches up with the frontmatter date.

As an example, 23:00 on the east coast is 04:00 of the next day UTC. Let's say the frontmatter date is 2024-01-15. On the east coast the OS will show it as 2024-01-15 23:00 but in UTC it will be 2024-01-16 04:00. We don't want the date fix to update the frontmatter to 2024-01-16 so the operational_timezone ensures that it's looking at the UTC date from the OS as if it's in the east coast to compare it to what's int he front matter - which will be 2024-01-15. 

### useful troubleshooting info
ok will output a list of any files that have invalid frontmatter.

ok will output any invalid wikilinks so your repo doesn't get messed up

### back populate behavior
Any existing wikilinks found in your markdown files will be back populated. Useful when you create a topic and want to get every instance in your repo that could target that new topic to have a link to it.

for example, if you create a new topic for [[OLED Displays]] and you already have a bunch of notes that refer to OLED Displays, then back populate will add links to the existing text. It's a useful search and replace.

If you have an alias in the wikilink such as [[OLED Displays|OLED]], then OLED will also become a target for replacing with [[OLED Displays|OLED]] so it will still render as OLED in obsidian (but now with a link)

If you have linked text but haven't created the note then no note will be created but other text that matches that link will also get the link attached.

Every .md page in your repo will also get added as a wikilink to back populate in case you haven't already linked them up.

If you have the property "aliases" in your markdown frontmatter, they will also be created as links that can be back-populated.  For example, this is the frontmatter for a page named sugar.md

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

because of the potential for edge cases i haven't thought of - you can run ok in dry run mode with apply_changes set to false so you can verify the changes before they happen.

#### ambiguous wikilinks
if two different pages have the same alias - for example, if you have pages for people and they have the same first name which you use as an alias, then back population can find two different target pages for the same text. 

because of this, ok will not replace these with wikilinks but instead will show them to you so you can take action and change them to whichever target you wish.  

ok will protect you!
### images
images are hashed to determine whether there are file duplicates. if there are, then one will be chosen to be kept and the rest will be deleted and any references to the deleted images will be updated to point at the one that is kept.

this may or may not work for you and it is not currently configurable so you'll either need to fork the code and remove this functionality or wait for me to make it a configurable capability.

Any images that are not referenced by files will be deleted - very destructive!

Any images that can't render (TIFF, Zero-Byte length files) will be deleted - very destructive!

## configuration details

### obsidian_path

Required. Path to your Obsidian vault. Supports shell expansion using `~` for home directory.

### output_folder

Required. Location for the "obsidian knife output.md" file. Path is relative to obsidian_path. 

This output folder will be automatically added to ignore_folders. As such it's a convenient place for you to store your configuration.md file if you wish. 

### apply_changes

Optional. Default: false
- false: dry-run mode, only shows proposed changes
- true: applies all changes shown in output file

if you have this configuration.md (or whatever you name it) in obsidian, then the apply_changes will output as a radio button you can click to enable. 

After ok does an update with apply_changes: true, it will set this property back to false so you don't accidentally apply changes when you may not want to - especially when making sure that things work.

### back_populate_file_count

Optional. Limits the number of files processed for back population. Useful for testing changes on a subset of files. 

For example, you can set apply_changes: false and then limit the number of files processed so you can assess if ok is doing the right thing.  once your happy with the results, you can either remove this property or set it to a very large number.

### back_populate_file_filter

Optional. Process only a specific file for back population. Value can be in wikilink format (`[[note]]`) or plain text (`note.md`). Useful for debugging.

### do_not_back_populate

Optional. List of text patterns to exclude from back population. Useful for:
- Common phrases that should not become wikilinks
- Text that renders the same as a note title or alias but shouldn't be linked

Each pattern is matched case-insensitively as a complete word.

In my repo i have a file for a friend named Will. The file is his full name but Will is an alias.  I don't want the word Will to be turned into [[Will A Friend|Will]] everywhere so will one of my do_not_back_populate entries in my config.

do_not_back_populate is special in that you can also add it as a yaml property on any of your pages to prevent substituting wikilinks just on that page

### ignore_folders

Optional. List of folders to skip during processing. Paths are relative to obsidian_path. The output_folder from the configuration file, `.obsidian`  and `.obsidian_knife` are automatically added to this list.

## cache

ok creates a `.ok` folder in your vault to store image hashes. This cache improves performance when checking for duplicate images across multiple runs. Especially in larger repos.
