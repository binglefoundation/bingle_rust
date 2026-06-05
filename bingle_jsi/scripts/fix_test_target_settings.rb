#!/usr/bin/env ruby
# Fix build settings for BingleJsiBridgeTests target.
require 'xcodeproj'

PROJ_PATH = File.expand_path('../example/ios/BingleJsiExample.xcodeproj', __dir__)
TEST_TARGET_NAME = 'BingleJsiBridgeTests'

project = Xcodeproj::Project.open(PROJ_PATH)
test_target = project.targets.find { |t| t.name == TEST_TARGET_NAME }
raise "Could not find target '#{TEST_TARGET_NAME}'" unless test_target

test_target.build_configurations.each do |config|
  config.build_settings['PRODUCT_NAME'] = TEST_TARGET_NAME
  config.build_settings['PRODUCT_BUNDLE_IDENTIFIER'] = 'com.bingle.BingleJsiBridgeTests'
  config.build_settings['SWIFT_VERSION'] = '5.0'
  # Must match the minimum deployment target of the BingleJsiRN pod (iOS 15.1)
  config.build_settings['IPHONEOS_DEPLOYMENT_TARGET'] = '15.1'
  config.build_settings['CODE_SIGN_STYLE'] = 'Automatic'
  config.build_settings['CODE_SIGNING_ALLOWED'] = 'NO'
  config.build_settings['GENERATE_INFOPLIST_FILE'] = 'YES'
  # Run as a standalone unit test bundle (no host app required).
  # The test target links directly against the BingleJsiRN library via Pods,
  # so all bridge types are available without needing the RN app to launch.
  config.build_settings['TEST_HOST'] = ''
  config.build_settings['BUNDLE_LOADER'] = ''
  puts "Updated config: #{config.name}"
end

# Remove the host app dependency so the test bundle can run standalone
dep = test_target.dependencies.find { |d| d.target&.name == 'BingleJsiExample' }
dep&.remove_from_project

project.save
puts "Saved #{PROJ_PATH}"
