---
obsidian_path: ~/Documents/brain
output_folder: conf/obsidian_knife

apply_changes: false

# file in the repo that you specifically want to process for back populating - used for debugging purposes
back_populate_file_filter: [[some markdown]]

do_not_back_populate: 
  - "[[mozzarella]] cheese"
  
# count of files to actually process - mostly for debugging purposes in case you want to find out
# what replacements are going to happen and you're seeing to many files
file_limit: 1

ignore_folders:
  - templates

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

---
apply_changes: false
do_not_back_populate:
  - away
  - bill
  - ed
  - jim
  - ok
  - people
  - tom
  - will
file_limit: 2000
ignore_folders:
  - .idea
  - conf/templates
obsidian_path: ~/Documents/brain
operational_timezone: America/New_York
output_folder: conf/obsidian_knife
tags: [code]
---

this file is a working markdown file example see the repo readme.md for explanation of config parameters
