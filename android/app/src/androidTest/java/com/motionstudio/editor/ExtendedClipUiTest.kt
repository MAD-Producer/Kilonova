package com.motionstudio.editor

import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.*
import org.junit.Assert.*
import java.io.File
import java.util.UUID

class ExtendedClipUiTest {
    @get:Rule val compose=createEmptyComposeRule()
    @Test fun signedSpanMovesAndUndoKeepsStoredBoundsAndTimelineVisible() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val root=File(context.filesDir,"acceptance/extended-clip-ui-${UUID.randomUUID()}").apply{mkdirs()}
        val p=LayerClipTimelineTest().fixture().put("version",13).put("frames",24)
        val layer=p.getJSONArray("layers").getJSONObject(0)
        layer.getJSONObject("timeline").put("in_frame",-6).put("out_frame",36).put("offset_frame",-6)
        layer.getJSONObject("transform").getJSONObject("rotation").put("keys",org.json.JSONArray())
        File(root,"project.json").writeText(p.toString())
        val scenario=ActivityScenario.launch<AcceptanceActivity>(Intent(context,AcceptanceActivity::class.java).putExtra("projectDirectory",root.absolutePath))
        lateinit var vm:EditorViewModel
        try {
            scenario.onActivity{vm=ViewModelProvider(it)[EditorViewModel::class.java]}
            compose.waitUntil(20000){vm.state.project!=null&&vm.state.sample?.optJSONObject("capabilities")!=null}
            assertNull(vm.state.error);compose.onNodeWithTag("timeline").assertIsDisplayed();compose.onNodeWithTag("preview-gesture").assertIsDisplayed()
            scenario.onActivity{vm.startLayerSelection(2)}
            assertTrue(vm.selectedClipDeltaRange()!!.contains(-10));assertEquals(-36000..36000,vm.clipSpanRange())
            scenario.onActivity{assertTrue(vm.moveSelectedClips(-10))}
            compose.waitUntil(15000){vm.state.saved&&vm.timelineLayer(2)?.optInt("in_frame")==-16}
            assertEquals(26,vm.timelineLayer(2)!!.getInt("out_frame"));assertEquals(-16,vm.timelineLayer(2)!!.getInt("offset_frame"))
            scenario.onActivity{vm.undo()}
            compose.waitUntil(15000){vm.state.saved&&vm.timelineLayer(2)?.optInt("in_frame")==-6}
            assertEquals(36,vm.timelineLayer(2)!!.getInt("out_frame"))
            compose.waitForIdle();val shot=InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
            File(root,"extended-clip-timeline.png").outputStream().use{shot.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)};shot.recycle()
            assertEquals(-6,JSONObject(File(root,"project.json").readText()).getJSONArray("layers").getJSONObject(0).getJSONObject("timeline").getInt("in_frame"))
            assertNull(vm.state.error)
        }finally{scenario.close()}
    }
}
