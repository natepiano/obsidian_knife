---
obsidian_path: ~/Documents/brain
output_folder: conf/obsidian_knife

apply_changes: false

ignore_folders:
  - .idea
  - .obsidian
  - conf/templates

ignore_text:
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
