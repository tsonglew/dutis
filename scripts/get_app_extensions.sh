#!/bin/bash

# Find all .app bundles in common application directories.
# Remove -onlyin flags to search the entire system (slower).
app_paths=$(mdfind "kMDItemContentType == 'com.apple.application-bundle'" -onlyin /System/Applications -onlyin /Applications)

# Loop through each application path
while IFS= read -r app_path; do
  info_plist_path="$app_path/Contents/Info.plist"

  # Check if the Info.plist file exists and is readable
  if [ -f "$info_plist_path" ]; then
    
    # Check if CFBundleDocumentTypes key exists before trying to process it
    # The command returns the key if it exists, otherwise it's empty.
    has_doc_types=$(/usr/libexec/PlistBuddy -c "Print :CFBundleDocumentTypes" "$info_plist_path" 2>/dev/null)
    
    if [ -n "$has_doc_types" ]; then
      
      all_extensions=""
      
      # Determine the number of document types defined for the app
      # We achieve this by trying to print the key and counting the "Dict" entries which represent each document type.
      doc_type_count=$(/usr/libexec/PlistBuddy -c "Print :CFBundleDocumentTypes" "$info_plist_path" 2>/dev/null | grep -c "Dict")

      if [[ $doc_type_count -gt 0 ]]; then
        # Loop through each document type dictionary (from 0 to count-1)
        for i in $(seq 0 $((doc_type_count - 1))); do
          
          # For each document type, try to print its extension array.
          # The output from PlistBuddy for an array can be messy, so we clean it up.
          # 1. Print the array :CFBundleDocumentTypes:i:CFBundleTypeExtensions
          # 2. Remove lines containing "Array {" and "}"
          # 3. Remove leading/trailing whitespace and extra spaces
          # 4. Replace newlines with spaces for a single line output
          extensions_for_type=$(/usr/libexec/PlistBuddy -c "Print :CFBundleDocumentTypes:$i:CFBundleTypeExtensions" "$info_plist_path" 2>/dev/null | grep -v "Array {" | tr -d ' }' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | tr '\n' ' ')
          
          # Append the found extensions to our master list for this app
          if [ -n "$extensions_for_type" ]; then
            all_extensions+="$extensions_for_type "
          fi
        done
      fi

      # If after checking all document types, we found any extensions, print them.
      if [ -n "$all_extensions" ]; then
        echo "----------------------------------------"
        echo "Application: $(basename "$app_path")"
        echo "----------------------------------------"
        # Use xargs to trim leading/trailing whitespace and normalize spaces
        echo "  Supported Extensions: $(echo $all_extensions | xargs)"
        echo ""
      fi
    fi
  fi
done <<< "$app_paths"