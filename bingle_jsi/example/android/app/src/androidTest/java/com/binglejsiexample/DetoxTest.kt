package com.binglejsiexample

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.rule.ActivityTestRule
import com.wix.detox.Detox
import com.wix.detox.config.DetoxConfig
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Detox instrumentation entry point (issue #130). Detox drives the app through this single
 * instrumentation test: it launches MainActivity and hands control to the Detox test runner, which
 * runs the JavaScript e2e suites (the e2e test files) against the app over the debug bridge.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class DetoxTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    @Test
    fun runDetoxTests() {
        val config = DetoxConfig()
        config.idlePolicyConfig.masterTimeoutSec = 90
        config.idlePolicyConfig.idleResourceTimeoutSec = 60
        Detox.runTests(activityRule, config)
    }
}
