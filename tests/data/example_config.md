---
obsidian_path: ~/Documents/brain
output_folder: conf/obsidian_knife

apply_changes: false

# count of files to actually process - mostly for debugging purposes in case you want to find out
# what replacements are going to happen and you're seeing to many files
back_populate_file_count: 1

# file in the repo that you specifically want to process for back populating - used for debugging purposes
back_populate_file_filter: [[some markdown]]

do_not_back_populate: 
  - "[[mozzarella]] cheese"

ignore_folders:
  - .idea
  - .obsidian
  - conf/templates

ignore_rendered_text:
 - "Ed: music reco:"

# anything that renders as the specified string will get replaced with the specified string
# for example [[foo|Ed]]: would render to Ed: so the link part would get replaced with Ed so
# the remaining string would be Ed:
#
# it has come up because there are people named Ed but i also ed: to indicate editorial
simplify_wikilinks:
 - "Ed:"
 - "Bob Rock"
---

this is an example configuration file. I thought it would be helpful to just store the config as a markdown file in obsidian
so it lives with the repository

to make it work, just copy/paste the configuration parameters and put it into a markdown file in 
obsidian as the frontmatter - placed between two lines with just --- in them like so (without the comments)

```yaml
 ---
 <your config goes here>
 ---
```
