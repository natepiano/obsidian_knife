# Here's a plan for migrating away from GroupedImages towards using ImageFile states and filter_by_variant:

# Phase 1: Reports Migration - done
Update each report that uses group_images to instead work with ImageFiles directly - done

# Phase 2: Updates to analyze_images
Remove all GroupedImages usage in analyze_images()
Use filter_by_variant for all operations
Keep generating GroupedImages temporarily but only for reports
Add tests to verify operations are identical

# Phase 3: Remove GroupedImages
After all reports and analyze_images are converted:
Remove GroupedImages struct
Remove ImageGroupType enum
Remove group_images() function
Remove determine_image_group_type()

Update analyze_repository to not return GroupedImages
Update write_reports signature to not take GroupedImages
Clean up any remaining references

Would you like me to start with Phase 1 and show how to convert the first report?
