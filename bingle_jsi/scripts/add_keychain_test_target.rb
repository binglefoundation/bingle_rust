#!/usr/bin/env ruby
# Adds the BingleJsiKeychainTests target (and its shared scheme) to BingleJsiExample.xcodeproj.
#
# These tests use the real iOS Keychain through the Rust library. The simulator Keychain refuses a
# hostless test bundle (errSecMissingEntitlement, -34018), so unlike BingleJsiBridgeTests this target
# is hosted in the example app. Run once, then `pod install` in bingle_jsi/example/ios.
# Usage: ruby bingle_jsi/scripts/add_keychain_test_target.rb

require 'xcodeproj'

PROJ_PATH = File.expand_path('../example/ios/BingleJsiExample.xcodeproj', __dir__)
TEST_TARGET_NAME = 'BingleJsiKeychainTests'
TEST_FILES_DIR = File.expand_path("../example/ios/#{TEST_TARGET_NAME}", __dir__)
APP_TARGET_NAME = 'BingleJsiExample'
BUNDLE_ID = 'com.bingle.BingleJsiKeychainTests'

project = Xcodeproj::Project.open(PROJ_PATH)

if project.targets.any? { |t| t.name == TEST_TARGET_NAME }
  puts "Target '#{TEST_TARGET_NAME}' already exists — nothing to do."
  exit 0
end

app_target = project.targets.find { |t| t.name == APP_TARGET_NAME }
raise "Could not find app target '#{APP_TARGET_NAME}'" unless app_target

test_target = project.new_target(:unit_test_bundle, TEST_TARGET_NAME, :ios, '15.1')
test_target.add_dependency(app_target)

test_target.build_configurations.each do |config|
  config.build_settings['PRODUCT_NAME'] = TEST_TARGET_NAME
  config.build_settings['PRODUCT_BUNDLE_IDENTIFIER'] = BUNDLE_ID
  config.build_settings['SWIFT_VERSION'] = '5.0'
  # Must match the minimum deployment target of the BingleJsiRN pod (iOS 15.1)
  config.build_settings['IPHONEOS_DEPLOYMENT_TARGET'] = '15.1'
  config.build_settings['CODE_SIGN_STYLE'] = 'Automatic'
  config.build_settings['GENERATE_INFOPLIST_FILE'] = 'YES'
  config.build_settings['TEST_HOST'] = "$(BUILT_PRODUCTS_DIR)/#{APP_TARGET_NAME}.app/$(BUNDLE_EXECUTABLE_FOLDER_PATH)/#{APP_TARGET_NAME}"
  config.build_settings['BUNDLE_LOADER'] = '$(TEST_HOST)'
end

tests_group = project.main_group.find_subpath(TEST_TARGET_NAME, true)
tests_group.set_source_tree('<group>')
tests_group.set_path(TEST_TARGET_NAME)

Dir.glob(File.join(TEST_FILES_DIR, '*.swift')).sort.each do |swift_file|
  file_ref = tests_group.new_reference(File.basename(swift_file))
  file_ref.set_source_tree('<group>')
  test_target.source_build_phase.add_file_reference(file_ref)
  puts "Added source file: #{File.basename(swift_file)}"
end

project.save

scheme = Xcodeproj::XCScheme.new
scheme.add_build_target(test_target, false)
scheme.add_test_target(test_target)
scheme.set_launch_target(app_target)
scheme.save_as(PROJ_PATH, TEST_TARGET_NAME, true)

puts "Saved #{PROJ_PATH} with target and shared scheme '#{TEST_TARGET_NAME}'."
