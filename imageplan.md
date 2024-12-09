current files needed (to start)
- obsidian_repository
- obsidian_repository_types
- image_file
- image_files


# Obsidian Knife Image Management Refactoring Plan

## Goals
1. Eliminate duplicate image processing logic
2. Move image processing upstream to be part of line-by-line parsing and updating
   - specifically we want to get away from using MarkdownOperation and removing/replacing image references based on a whole page match
   - but rather replace them similar to how we replace missing image links where we have the position and line information
   - the intent is to move things upstream
3. Create cleaner, more maintainable image management system
4. Improve performance by reducing redundant operations

## not to forget:
- use existing APIs and fn's rather than imagining new ones that don't exist
- don't create duplicative code on match arms that are essentially doing the same thing

## Completed Steps

### Phase 1: Foundation ✓
- Created image_file.rs with new types
- Added ImageFile struct and related types
- Added tests in image_file_tests.rs

### Phase 2: Basic Structure ✓
- Created ImageFiles struct for managing collections
- Modified ObsidianRepository to support new structures
- Updated ObsidianRepository::new()
- Added support for both old and new structures

## Remaining Steps

### Phase 3: Core Logic Migration
1. Enhance ImageFile
   - Move classification logic from determine_image_group_type
   - Add state management methods
   - Add methods for reference tracking

2. Create parallel implementation
   - Add new analysis methods using ImageFiles/ImageFile
   - Ensure feature parity with existing implementation
   - Use ImageState from ImageFile to drive the grouping
   - let ImageFiles handle the collection management
   - convert to the existing types (GroupedImages, ImageOperations) only at the end
   - if different match arms use the same logic that can be parameterized, make a new fn
   - Run both implementations for comparison testing

3. Add comprehensive tests
   - Compare results between implementations
   - Test edge cases and special scenarios
   - Verify no functionality loss

4.  determine whether we can get rid of analyze_images call flow and switch to analyze_images_new
   - Understand what other parts of the codebase might be depending on the old implementation
   - Make sure the new implementation fully handles all edge cases that the old one did - seemingly it must as all tests pass
   - Ideally add more tests to cover any scenarios we might have missed
   - Consider if there are any performance implications

### Phase 4: Transition
1. Start using new implementation in ObsidianRepository
2. Deprecate but maintain old methods during transition
3. Update all dependent code to use new types
4. Add migration tests to verify behavior consistency

5. to be planned / designed - move MarkdownOperations for update reference and remove reference to occur upstream
   should an ImageLink referring to a local file have a ImageFile created for it if it's not a missing file? 
   we've already scanned for MarkdownFile and assembled all of its image links but how do we connect through to the actual file itself?

### Phase 5: Cleanup
1. Remove deprecated types and methods:
   - ImageGroupType
   - ImageReferences
   - group_images() old version
   - determine_group_type()
   - image_path_to_references_map

2. Remove transition code:
   - Delete type conversion code
   - Remove parallel implementation checks
   - Clean up unused dependencies

3. Final verification:
   - Run full test suite
   - Verify performance metrics
   - Document any API changes
