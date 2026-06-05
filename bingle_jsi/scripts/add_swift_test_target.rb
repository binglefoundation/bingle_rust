#!/usr/bin/env ruby
# Adds the BingleJsiBridgeTests Swift unit test target to BingleJsiExample.xcodeproj.
# Run this script once from the bingle_jsi/example/ios directory.
# Usage: /opt/homebrew/Cellar/cocoapods/1.16.2_1/libexec/bin/ruby add_swift_test_target.rb

require 'xcodeproj'

PROJ_PATH = File.expand_path('../example/ios/BingleJsiExample.xcodeproj', __dir__)
TEST_TARGET_NAME = 'BingleJsiBridgeTests'
TEST_FILES_DIR = File.expand_path('../example/ios/BingleJsiBridgeTests', __dir__)
APP_TARGET_NAME = 'BingleJsiExample'
BUNDLE_ID = 'com.bingle.BingleJsiBridgeTests'

project = Xcodeproj::Project.open(PROJ_PATH)

# Skip if already added
if project.targets.any? { |t| t.name == TEST_TARGET_NAME }
  puts "Target '#{TEST_TARGET_NAME}' already exists — nothing to do."
  exit 0
end

# Create the unit test target
test_target = project.new_target(
  :unit_test_bundle,
  TEST_TARGET_NAME,
  :ios,
  '14.0'
)

# Find the app target to set as the host (required for a unit test bundle)
app_target = project.targets.find { |t| t.name == APP_TARGET_NAME }
raise "Could not find app target '#{APP_TARGET_NAME}'" unless app_target

test_target.add_dependency(app_target)

# Configure build settings
test_target.build_configurations.each do |config|
  config.build_settings['PRODUCT_BUNDLE_IDENTIFIER'] = BUNDLE_ID
  config.build_settings['SWIFT_VERSION'] = '5.0'
  config.build_settings['IPHONEOS_DEPLOYMENT_TARGET'] = '14.0'
  config.build_settings['TEST_HOST'] = "$(BUILT_PRODUCTS_DIR)/#{APP_TARGET_NAME}.app/$(BUNDLE_EXECUTABLE_FOLDER_PATH)/#{APP_TARGET_NAME}"
  config.build_settings['BUNDLE_LOADER'] = '$(TEST_HOST)'
  config.build_settings['CODE_SIGN_STYLE'] = 'Automatic'
  config.build_settings['GENERATE_INFOPLIST_FILE'] = 'YES'
end

# Create a group for the test sources
tests_group = project.main_group.find_subpath(TEST_TARGET_NAME, true)
tests_group.set_source_tree('<group>')
tests_group.set_path(TEST_TARGET_NAME)

# Add the Swift source files to the group and the target's Sources build phase
Dir.glob(File.join(TEST_FILES_DIR, '*.swift')).each do |swift_file|
  filename = File.basename(swift_file)
  file_ref = tests_group.new_reference(filename)
  file_ref.set_source_tree('<group>')
  test_target.source_build_phase.add_file_reference(file_ref)
  puts "Added source file: #{filename}"
end

project.save
puts "Saved #{PROJ_PATH}"
puts "Target '#{TEST_TARGET_NAME}' added successfully."
puts ""
puts "Next steps:"
puts "  1. Update the Podfile to add pods for the new target."
puts "  2. Run: pod install"
