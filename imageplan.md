current files needed (to start)
- obsidian_repository
- obsidian_repository_types
- image_file
- image_files

# Obsidian Knife Image Management Refactoring Plan
we are in the middle of this large refactoring

## Goals
1. Eliminate duplicate image processing logic
2. Move image processing upstream to be part of line-by-line parsing and updating
   - Replace whole-page MarkdownOperation pattern with line-level processing
   - Process images during file scan, marking for different types of replacements
   Use ImageLink and ReplaceableContent for all image updates
3. Create cleaner, more maintainable image management system
4. Improve performance by reducing redundant operations

## not to forget:
- use existing APIs and fn's rather than imagining new ones that don't exist
- don't create duplicative code on match arms that are essentially doing the same thing
- ImageLink already implements ReplaceableContent - extend this rather than creating new types
- Use the same patterns as missing image handling for the Tiff, ZeroByte and Duplicate image reference cases

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

# Next Phases

## Phase 3: Parallel Implementation

### Structure Changes
#### Keep ✓
- `ImageFile`, `ImageFiles`, `ImageFileType`
- `ImageLinks`, `ImageLink`
- `GroupedImages`, `ImageGroup`, `ImageGroupType`

#### Modify ✓
- `ImageLink`: Add new states for duplicate/incompatible 
- `ReplaceableContent` implementation on `ImageLink`: Handle new states

#### Remove - not yet
- `MarkdownOperation`

### Implementation Steps
1. **Implement Parallel Processing**
   - introduce identify_image_reference_replacements which should correctly mark image_links so that an updated apply_replaceable_matches will include them for replacement in line by line replacement
   - update tests to stop using MarkdownOperation and instead look at the outcome - similar to missing references tests
   - compare that the apply_replaceable_matches has actually already done the MarkdownOperations that have been requested and if so we can remove MarkdownOperation entirely - but we have to handle tiff, zerobyte, duplicate and missing all correctly

2. **Testing Framework**
   - Create any additional tests
   - Test corner cases: empty files, multiple references, nested paths
   - Validate file content changes match exactly

3. **Migration**
   - Once validation passes, remove old MarkdownOperation code path
   - Update any dependent code to use new ImageLink states
   - Final pass of tests with only new implementation

4. ** Move to using ImageFile **
   - we created ImageFile earlier in the process and in theory it should have the information necessary so we don't
   - have to use grouped images anymore - is that possible?
   - let's try to add new analysis methods using ImageFiles/ImageFile
   - Ensure feature parity with existing implementation
   - Use ImageState from ImageFile to drive the grouping
   - let ImageFiles handle the collection management
   - if different match arms use the same logic that can be parameterized, make a new fn
   - Run both implementations for comparison testing

## Phase 4: Documentation & Clean Up

1. **Documentation Updates**
   - Update documentation to reflect new approach
   - Document new ImageLink states and their handling
   - Update examples and usage patterns

2. **Code Cleanup**
   - Remove temporary comparison code
   - Remove unused imports and dead code
   - Final review of error handling and edge cases

3. **Final Verification**
   - Verify all test cases still pass
   - Confirm no regressions in functionality
   - Validate performance metrics
