package com.motionstudio.editor

import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.UUID

class SubframeClipUiTest {
    @get:Rule val compose=createEmptyComposeRule()
    @Test fun precise_edges_survive_moves_trim_editor_and_undo() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val root=File(context.filesDir,"acceptance/subframe-${UUID.randomUUID()}").apply{mkdirs()}
        val p=LayerClipTimelineTest().fixture().put("version",15).put("frames",24)
        val layer=p.getJSONArray("layers").getJSONObject(0)
        layer.getJSONObject("timeline").put("in_frame",-1).put("out_frame",37).put("offset_frame",-6)
        layer.put("subframe_range",JSONObject().put("in_frame",-0.25).put("out_frame",36.75))
        layer.getJSONObject("transform").getJSONObject("rotation").put("keys",org.json.JSONArray())
        File(root,"project.json").writeText(p.toString())
        val scenario=ActivityScenario.launch<AcceptanceActivity>(Intent(context,AcceptanceActivity::class.java).putExtra("projectDirectory",root.absolutePath))
        lateinit var vm:EditorViewModel
        try {
            scenario.onActivity{vm=ViewModelProvider(it)[EditorViewModel::class.java]}
            compose.waitUntil(20000){vm.state.project!=null&&vm.state.sample!=null}
            scenario.onActivity{vm.select(2,false);vm.seek(0.0)}
            compose.waitUntil(10000){vm.clipInterval(2)?.second==36.75}
            assertTrue(vm.preciseClipSupported());compose.onNodeWithTag("timeline").assertIsDisplayed()
            scenario.onActivity{vm.moveClip(2,9)}
            compose.waitUntil(10000){vm.state.saved&&vm.clipInterval(2)?.first==9.75}
            assertEquals(46.75,vm.clipInterval(2)!!.second,0.0)
            scenario.onActivity{vm.undo()}
            compose.waitUntil(10000){vm.state.saved&&vm.clipInterval(2)?.first==-.25}
            val density=context.resources.displayMetrics.density
            compose.onNodeWithTag("timeline").performTouchInput{longClick(Offset(width/2f+8*density,44*density+18*density))}
            compose.onNodeWithText("精确裁剪片段").performClick()
            compose.onNodeWithTag("clip-in").assertTextContains("-0.25").performTextReplacement("0.125")
            compose.onNodeWithTag("clip-out").assertTextContains("36.75").performTextReplacement("20.875")
            compose.onNodeWithText("应用裁剪").performClick()
            compose.waitUntil(10000){vm.state.saved&&vm.clipInterval(2)?.second==20.875}
            scenario.onActivity{vm.seek(10.0);vm.splitClip()}
            compose.waitUntil(10000){vm.state.saved&&vm.state.project!!.getJSONArray("layers").length()==2}
            val right=vm.state.project!!.getJSONArray("layers").getJSONObject(1).getLong("id")
            assertEquals(20.875,vm.clipInterval(right)!!.second,0.0)
            scenario.onActivity{vm.undo()}
            compose.waitUntil(10000){vm.state.saved&&vm.state.project!!.getJSONArray("layers").length()==1}
            val saved=JSONObject(File(root,"project.json").readText()).getJSONArray("layers").getJSONObject(0).getJSONObject("subframe_range")
            assertEquals(.125,saved.getDouble("in_frame"),0.0);assertEquals(20.875,saved.getDouble("out_frame"),0.0)
            assertNull(vm.state.error)
        } finally {scenario.close()}
    }
}
