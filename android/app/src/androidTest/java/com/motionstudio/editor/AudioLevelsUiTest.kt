package com.motionstudio.editor

import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.UUID
import java.nio.ByteBuffer
import java.nio.ByteOrder

class AudioLevelsUiTest {
    @get:Rule val compose=createEmptyComposeRule()
    private fun data(raw:String):JSONObject {val r=JSONObject(raw);assertTrue(r.optString("error"),r.optBoolean("ok"));return r.getJSONObject("data")}
    @Test fun native_stereo_tracks_reuse_wheels_timeline_keys_and_easing() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val root=File(context.filesDir,"acceptance/audio-levels-${UUID.randomUUID()}").apply{mkdirs()}
        val p=data(NativeBridge.projectTemplate(0)).put("width",256).put("height",256).put("fps",24).put("frames",48).put("layers",JSONArray())
        val native=NativeBridge.create(root.absolutePath,p.toString());assertTrue(native>0)
        fun request(op:String,vararg values:Pair<String,Any>)=data(MediaBridge.request(native,context,
            JSONObject().put("op",op).apply{values.forEach{put(it.first,it.second)}}.toString()))
        try {
            request("import_media","request_id" to "tone","kind" to "audio","at_frame" to 0,
                "uri" to "content://com.motionstudio.editor.test.audio-fixtures/tone-stereo-48000.wav")
            val deadline=System.nanoTime()+30_000_000_000L
            while(request("media_status","request_id" to "tone").getString("state")=="running"){
                assertTrue("Audio fixture import timeout",System.nanoTime()<deadline);Thread.sleep(10)
            }
            request("finish_media_import","request_id" to "tone")
            fun pcm():FloatArray {
                val buffer=ByteBuffer.allocateDirect(512*8).order(ByteOrder.LITTLE_ENDIAN)
                assertEquals(512,data(MediaBridge.readPcmInto(native,12000,512,buffer)).getInt("frames"))
                return FloatArray(1024).also{buffer.asFloatBuffer().get(it)}
            }
            val baseline=pcm();val objectId=data(NativeBridge.state(native)).getJSONObject("project").getJSONArray("layers").getJSONObject(0).getLong("id")
            data(NativeBridge.command(native,JSONObject().put("op","audio_levels").put("object",objectId)
                .put("action",JSONObject().put("kind","set").put("channel","left").put("frame",0).put("value",-6)).toString()))
            val adjusted=pcm()
            assertTrue(baseline.any{kotlin.math.abs(it)>.01f})
            for(i in 0 until 512) {
                assertEquals(baseline[2*i]*Math.pow(10.0,-6.0/20.0).toFloat(),adjusted[2*i],1e-6f)
                assertEquals(baseline[2*i+1],adjusted[2*i+1],1e-6f)
            }
            data(NativeBridge.command(native,JSONObject().put("op","audio_levels").put("object",objectId)
                .put("action",JSONObject().put("kind","clear")).toString()))
            data(NativeBridge.save(native))
        } finally {NativeBridge.destroy(native)}
        val scenario=ActivityScenario.launch<AcceptanceActivity>(Intent(context,AcceptanceActivity::class.java).putExtra("projectDirectory",root.absolutePath))
        lateinit var vm:EditorViewModel
        try {
            scenario.onActivity{vm=ViewModelProvider(it)[EditorViewModel::class.java]}
            compose.waitUntil(20000){vm.state.project!=null&&vm.state.sample!=null&&vm.importTask==null}
            val objectId=vm.state.project!!.getJSONArray("layers").getJSONObject(0).getLong("id")
            scenario.onActivity{vm.select(objectId,false);vm.property="audio";vm.panelOpen=true;vm.seek(0.0)}
            compose.onNodeWithTag("audio-level-wheel-left").assertIsDisplayed()
            compose.onNodeWithTag("audio-level-wheel-right").assertIsDisplayed()
            compose.onNodeWithTag("audio-level-select-left").performClick()
            compose.onNodeWithContentDescription("音量关键帧").performClick()
            compose.waitUntil(10000){vm.propertyTrack()?.getJSONArray("keys")?.length()==1&&vm.state.saved}
            scenario.onActivity{vm.seek(12.0)}
            compose.waitUntil(10000){vm.state.sample?.optDouble("frame")==12.0}
            scenario.onActivity{vm.setValue(-12.0)}
            compose.waitUntil(10000){vm.propertyTrack()?.getJSONArray("keys")?.length()==2&&vm.state.saved}
            scenario.onActivity{vm.seek(6.0)}
            compose.waitUntil(10000){vm.state.sample?.optDouble("frame")==6.0}
            assertEquals(-6.0,vm.audioLevelValue(objectId,"audio:left")!!,1e-6)
            assertEquals(0.0,vm.audioLevelValue(objectId,"audio:right")!!,1e-6)
            compose.onNodeWithTag("audio-level-curve").performClick()
            compose.onNodeWithText("返回声音参数").performClick()
            scenario.onActivity{vm.undo()}
            compose.waitUntil(10000){vm.state.saved&&vm.audioLevelValue(objectId,"audio:left")==0.0}
            scenario.onActivity{vm.redo()}
            compose.waitUntil(10000){vm.state.saved&&vm.audioLevelValue(objectId,"audio:left")==-6.0}
            assertEquals(14,vm.state.project!!.getInt("version"));assertNull(vm.state.error)
            val saved=JSONObject(File(root,"project.json").readText())
            assertEquals(2,saved.getJSONArray("layers").getJSONObject(0).getJSONObject("audio_levels").getJSONObject("left").getJSONArray("keys").length())
            val shot=InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
            File(root,"audio-levels-ui.png").outputStream().use{shot.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)};shot.recycle()
        } finally {scenario.close()}
    }
}
