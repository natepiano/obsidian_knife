# obsidian_knife
cli utility to manage my [obsidian](https://obsidian.md) folder

## capabilities
- configurable dry-run mode
- outputs changes to a file in a specified folder
- simplify unintentionally created wikilinks**
- deduplicate images
- remove local images and image references that won't render such as
    - tiff images
    - zero byte images
    - references to non-existent images

### ** simplify unintentional wikilinks
Q: what does this mean?

A: when automatically creating wikilinks from text that matches an existing note in obsidian, sometimes we get links that we don't want.
For example, i some times use the abbreviation Ed: or ed: to indicate "editorial". I also have a couple of friends named Ed.  Wikilinks for my friend might look like this:

```
[[Ed Smith|Ed]] or [[Ed Jones|Ed]] 
```

When i apply new links to existing text, I look for both the main link (i.e. "Ed Smith") but i also 
search for the alias (i.e. "Ed"). My first algo naively replaced 
```
Ed:

with

[[Ed Smith|Ed]]:
```

now i can create just a simple search/replace utility but i'd have to capture a few variations or get more clever with my regex. and maybe a future version i will do so. 
but for now, i created the simplify wikilinks to "undo" any unintentional creations. A similar problem exists with any friend 
named Will - where Will is one of the aliases. The word shows up a lot.  

# usage 
there is an example config.yaml in the root of this repo - the contents of this yaml should be placed in a markdown file wherever you want in your
obsidian repo. I recommend in the same folder you specify as the output_folder.
then run the command line and path the path to the config yaml - ~ replacement for $HOME will work

an output file named "obsidian knife output.md" will be created in this folder where you can preview changes

if you want to apply changes then change apply_changes to true. if you have placed the config.md (or whatever you name it) as a md file in obsidian, 
then obsidian will see apply_changes as a boolean that you can change by clicking on a rendered radio button.

# todo
- replace create date on markdown file using defined property - also update date_created property in frontmatter
- apply new links to existing text  
