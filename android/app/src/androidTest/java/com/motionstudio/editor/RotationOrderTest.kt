package com.motionstudio.editor

import android.content.Intent
import android.graphics.BitmapFactory
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
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.UUID

class RotationOrderTest {
    @get:Rule val compose=createEmptyComposeRule()
    private fun data(raw:String)=nativeData(raw)
    private fun root()=File(InstrumentationRegistry.getInstrumentation().targetContext.filesDir,
        "acceptance/rotation-order-${UUID.randomUUID()}").apply{mkdirs()}
    private fun fixture():JSONObject {
        val p=data(NativeBridge.projectTemplate(0)).put("width",256).put("height",256).put("frames",24).put("fps",24)
            .put("background",JSONArray(listOf(0,0,0,1)))
        p.getJSONObject("camera").put("created",false)
        val layer=p.getJSONArray("layers").getJSONObject(0).put("id",1).put("three_d",true)
            .put("size",JSONArray(listOf(160,80))).put("parent",JSONObject.NULL)
            .put("content",JSONObject().put("kind","solid").put("color",JSONArray(listOf(1,1,1,1))))
        layer.remove("rotation_order")
        val t=layer.getJSONObject("transform")
        for((name,value) in listOf("position" to listOf(128,128,0),"rotation" to listOf(30,40,50),"scale" to listOf(100,100,100)))
            t.getJSONObject(name).put("value",JSONArray(value)).put("keys",JSONArray()).remove("axes")
        t.getJSONObject("opacity").put("value",1).put("keys",JSONArray())
        p.put("layers",JSONArray().put(layer))
        return p
    }
    private fun command(id:Long,op:String,vararg args:Pair<String,Any>)=data(NativeBridge.command(id,
        JSONObject().put("op",op).apply{args.forEach{put(it.first,it.second)}}.toString()))

    @Test fun animated_fixed_axes_match_gles_and_survive_history_and_reopen() {
        val root=root();val id=NativeBridge.create(root.absolutePath,fixture().toString());assertTrue(NativeBridge.creationError(),id>0)
        try {
            val original=data(NativeBridge.state(id)).getJSONObject("project").toString()
            val legacy=BitmapFactory.decodeFile(data(NativeBridge.capture(id)).getString("path"))
            command(id,"set_rotation_order","object" to 1,"order" to "fixed_xyz")
            val fixed=BitmapFactory.decodeFile(data(NativeBridge.capture(id)).getString("path"))
            try{assertFalse("Angles must distinguish rotation orders",legacy.sameAs(fixed))}finally{legacy.recycle();fixed.recycle()}
            data(NativeBridge.history(id,0));assertEquals(original,data(NativeBridge.state(id)).getJSONObject("project").toString())
            data(NativeBridge.history(id,1))
            command(id,"animate","object" to 1,"property" to "rotation","frame" to 0,"enabled" to true)
            command(id,"set_vector","object" to 1,"property" to "rotation","frame" to 23,"value" to JSONArray(listOf(-25,15,80)))
            val p=data(NativeBridge.state(id)).getJSONObject("project");assertEquals(16,p.getInt("version"))
            val info=data(NativeBridge.renderPlanInfo(id));val reports=JSONArray()
            val gpu=EglMovieRenderer(null,256,256,p,id,info)
            try {for(frame in listOf(0,12,23,12)) {
                data(NativeBridge.seek(id,frame.toDouble()))
                val png=BitmapFactory.decodeFile(data(NativeBridge.capture(id)).getString("path"),BitmapFactory.Options().apply{inPremultiplied=false;inScaled=false})
                try {
                    val plan=ByteBuffer.allocateDirect(info.getInt("bufferBytes")).order(ByteOrder.LITTLE_ENDIAN)
                    assertTrue(NativeBridge.sampleRenderPlanInto(id,frame,plan)>0)
                    gpu.draw(plan);val pixels=ByteBuffer.allocateDirect(256*256*4);gpu.readPixelsInto(pixels)
                    reports.put(glesParity(png,pixels).put("frame",frame))
                } finally {png.recycle()}
            }}finally{gpu.close()}
            data(NativeBridge.save(id));File(root,"rotation-order-parity.json").writeText(reports.toString(2))
        } finally {NativeBridge.destroy(id)}
        val reopened=NativeBridge.create(root.absolutePath,"");assertTrue(reopened>0)
        try {
            val layer=data(NativeBridge.state(reopened)).getJSONObject("project").getJSONArray("layers").getJSONObject(0)
            assertEquals("fixed_xyz",layer.getString("rotation_order"))
            assertEquals(2,layer.getJSONObject("transform").getJSONObject("rotation").getJSONArray("keys").length())
        } finally {NativeBridge.destroy(reopened)}
    }

    @Test fun android_menu_switches_legacy_order_and_new_layers_use_fixed_axes() {
        val root=root();val context=InstrumentationRegistry.getInstrumentation().targetContext
        val native=NativeBridge.create(root.absolutePath,fixture().toString());assertTrue(native>0)
        try{data(NativeBridge.save(native))}finally{NativeBridge.destroy(native)}
        val scenario=ActivityScenario.launch<AcceptanceActivity>(Intent(context,AcceptanceActivity::class.java).putExtra("projectDirectory",root.absolutePath))
        lateinit var vm:EditorViewModel
        try {
            scenario.onActivity{vm=ViewModelProvider(it)[EditorViewModel::class.java]}
            compose.waitUntil(20000){vm.state.project!=null&&vm.state.sample!=null}
            scenario.onActivity{vm.select(1,false);vm.openProperty("rotation")}
            assertTrue(vm.rotationOrderSupported());assertFalse(vm.fixedRotationOrder())
            val tracks=vm.layer(1)!!.getJSONObject("transform").toString()
            compose.onNodeWithContentDescription("图层操作").performClick()
            compose.onNodeWithTag("rotation-order-toggle").performClick()
            compose.waitUntil(10000){vm.fixedRotationOrder()&&vm.state.saved}
            assertEquals(tracks,vm.layer(1)!!.getJSONObject("transform").toString())
            scenario.onActivity{vm.undo()};compose.waitUntil(10000){!vm.fixedRotationOrder()&&vm.state.saved}
            scenario.onActivity{vm.redo()};compose.waitUntil(10000){vm.fixedRotationOrder()&&vm.state.saved}
            scenario.onActivity{vm.addRectangle()}
            compose.waitUntil(10000){vm.selected!=1L&&vm.fixedRotationOrder()&&vm.state.saved}
            assertNull(vm.state.error)
            val p=JSONObject(File(root,"project.json").readText())
            assertEquals("fixed_xyz",p.getJSONArray("layers").getJSONObject(1).getString("rotation_order"))
        } finally {scenario.close()}
    }
}
