# obsidian_knife
cli utility to manage my [obsidian](https://obsidian.md) folder

## usage
Use example_config.md (tests/test/example_config.md) as your starting point.This config can be invoked anywhere, but I recommend you name it what you wish and
place it in your obsidian repo. I use a folder in my repo named conf and in that i have my templates folder - i also have a folder for output from obsidian_knife named,... "obsidian_knife"
I put the configuration file in this folder and named it conf.md.  

One of the parameters in the configuration yaml (the frontmatter section of example_config.md) - is "output_folder". If you specify the same folder as your output folder then when you run the CLI
you'll get the "obsidian knife output.md" in that same folder. 

example invocation:

```
obsidian_knife ~/Documents/my_obsidian_folder/conf/obsidian_knife/config.md
```

As mentioned, the output file "[obsidian_path]/[output_folder]/obsidian knife output.md" will be created where you can preview changes 

If you want to apply changes then change apply_changes to true. If you have placed the config.md (or whatever you name it) as a file in obsidian,
then obsidian will see the apply_changes field as a boolean that you can toggle by clicking on the rendered radio button.

## capabilities
- configurable apply_changes - set to false for a dry-run
- output changes to a file in a specified folder in your obsidian repo
- simplify unintentionally created wikilinks**
- deduplicate images 
- remove local images and image references that won't render such as
    - tiff images
    - zero byte images
    - references to non-existent images

### ** simplify unintentional wikilinks
Q: what does this mean?

A: when automatically creating wikilinks from text that matches an existing note in obsidian, sometimes we get links that we don't want.
For example, I use the abbreviation Ed: or ed: to indicate "editorial". I also have a couple of friends named Ed.  Wikilinks for my friend might look like this:

```
[[Ed Smith|Ed]] or [[Ed Jones|Ed]] 
```

When i use "apply new links to existing text" automation, the code looks for both the main link (i.e. "Ed Smith") but  
also searches for the alias (i.e. "Ed"). My first algo naively replaced 
```
Ed:
``` 
with
```
[[Ed Smith|Ed]]:
```

Of course i could create a simple search/replace utility but i'd have to capture a few variations or get more clever with my regex. and maybe a future version i will do so. 
but for now, i created the simplify wikilinks to "undo" any unintentional wikilink replacements. A similar problem exists with any friend 
named Will - where Will is one of the aliases. The word shows up a lot.

```
will you come to the store?
``` 

might get replaced with a case insensitive replacement a la:

```
[[Will Jones|Will]] you come to the store?
```

# yaml doc
```yaml
# false = dry-run, true=destructive 
apply_changes: false

# the number of files to process for back_populate
# remove this if you want it to populate all
back_populate_file_count: 1

# when back populating there are some strings you don't want to 
# be replaced, such as the cheese in [[mozzarella]] cheese
do_not_back_populate: 
  - "[[mozzarella]] cheese"

# add this value to name a property used in your md containing a file creation date you want to set
# if you set (for example) the date_creation property to a valid date in "[[yyyy-mm-dd]]" format then 
# the actual file creation date will be updated to this, and then the date_creation property will be removed from the md
creation_date_property: date_creation

# folders to ignore when scanning - this comes from my current actual configuration
ignore_folders:
- .idea
- .obsidian
- conf/templates

# when scanning markdown for replacements, ignore any with this exact text
ignore_rendered_text:
- "Ed: music reco:"

# path to obsidian folder - ~ welcome
obsidian_path: ~/Documents/obsidian_folder

# path to where you want the "obsidian knife output.md" file placed - defaults to "obsidian_knife"
output_folder: conf/obsidian_knife

# text you want to find that if it contains a wikilink, to then remove the wikilink from it
# i.e., "Bob [[Rock]]" would become "Bob Rock"
simplify_wikilinks:
- Ed:
- Bob Rock
```
# todo
- replace create date on markdown file using defined property - also update date_created property in frontmatter
- apply new links to existing text  
