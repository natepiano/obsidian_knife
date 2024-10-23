# obsidian_knife
cli utility to manage my [obsidian](https://obsidian.md) folder

## usage
there is an example_config.md (tests/test/example_config.md) - this config should be placed in a markdown file wherever you want in your
obsidian repo. I recommend in the same folder you specify as the output_folder for obsidian_knife. Then run the command line and path the path to the config yaml - ~ replacement for $HOME will work

i.e.:

```
obsidian_knife ~/Documents/my_obsidian_folder/obsidian_knife/config.md
```

an output file named "obsidian knife output.md" will be created in this folder where you can preview changes

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

# todo
- replace create date on markdown file using defined property - also update date_created property in frontmatter
- apply new links to existing text  
