package com.motionstudio.editor

import android.graphics.Bitmap
import android.graphics.Color
import android.hardware.HardwareBuffer
import android.media.ImageReader
import android.graphics.ImageFormat
import android.os.Handler
import android.os.HandlerThread
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

/** Exercises shared backend APIs and a real Surface; no editor UI interaction. */
@RunWith(AndroidJUnit4::class)
class PreviewMemoryBackendTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private fun data(raw: String) = nativeData(raw)
    private fun values(vararg values: Any) = JSONArray(values.toList())
    private fun track(value: Any) = JSONObject().put("value",value).put("keys",JSONArray())
    private fun layer(id: Int, size: Int, content: JSONObject, threeD: Boolean = false) = JSONObject()
        .put("id",id).put("name","memory $id").put("visible",true).put("locked",false).put("three_d",threeD)
        .put("size",values(size,size)).put("content",content)
        .put("transform",JSONObject().put("position",track(values(32,32,0)))
            .put("rotation",track(values(0,0,0))).put("scale",track(values(100,100,100)))
            .put("opacity",track(1)).put("anchor",values(.5,.5)))
    private fun create(root: File, assets: JSONArray, layers: JSONArray): Long {
        val p = data(NativeBridge.projectTemplate(0)).put("width",64).put("height",64)
            .put("fps",30).put("frames",60).put("assets",assets).put("layers",layers)
        p.getJSONObject("camera").put("created",false)
        return NativeBridge.create(root.absolutePath,p.toString()).also { assertTrue(NativeBridge.creationError(),it>0) }
    }
    private fun surface(id: Long, action: () -> Unit) {
        val consumer = HandlerThread("memory-backend-consumer").apply { start() }
        val reader = ImageReader.newInstance(64,64,ImageFormat.PRIVATE,3,HardwareBuffer.USAGE_GPU_SAMPLED_IMAGE)
        reader.setOnImageAvailableListener({it.acquireLatestImage()?.close()},Handler(consumer.looper))
        try { data(NativeBridge.surface(id,reader.surface,64,64)); action() }
        finally {
            data(NativeBridge.surface(id,null,0,0))
            reader.setOnImageAvailableListener(null,null)
            consumer.quitSafely(); consumer.join(3000); assertFalse(consumer.isAlive); reader.close()
        }
    }
    private fun awaitFrame(id: Long, frame: Double) {
        val until = System.nanoTime()+30_000_000_000L
        while(!NativeBridge.render(id,frame)) { assertTrue(NativeBridge.state(id),System.nanoTime()<until); Thread.sleep(5) }
    }
    @Test fun largeImageWorkingSetUsesSmallerProxiesWithoutEditingTheProject() {
        val root = File(context.filesDir,"memory-backend/${UUID.randomUUID()}").apply { mkdirs() }
        val file = File(root,"assets/source.png").apply { parentFile!!.mkdirs() }
        val bitmap = Bitmap.createBitmap(2048,2048,Bitmap.Config.ARGB_8888)
        try { bitmap.eraseColor(Color.GREEN); file.outputStream().use { assertTrue(bitmap.compress(Bitmap.CompressFormat.PNG,100,it)) } }
        finally { bitmap.recycle() }
        val assets = JSONArray(); val layers = JSONArray()
        for(id in 1..9) {
            assets.put(JSONObject().put("id",id).put("path","assets/source.png").put("width",2048).put("height",2048))
            layers.put(layer(id,64,JSONObject().put("kind","image").put("asset",id)))
        }
        val id = create(root,assets,layers)
        try { surface(id) {
            val before = data(NativeBridge.state(id))
            val project = before.getJSONObject("project").toString(); val revision = before.getLong("revision")
            awaitFrame(id,0.0)
            val info = data(NativeBridge.previewInfo(id)); val memory = info.getJSONObject("memoryAdaptation")
            assertEquals("degraded",memory.getString("status"))
            assertEquals(1024,memory.getJSONObject("actual").getInt("imageEdge"))
            assertEquals(32,memory.getInt("actualWidth")); assertEquals(32,memory.getInt("actualHeight"))
            assertEquals(1,memory.getInt("refusalCount")); assertFalse(memory.getBoolean("held"))
            val after = data(NativeBridge.state(id))
            assertEquals(project,after.getJSONObject("project").toString()); assertEquals(revision,after.getLong("revision"))
            assertEquals(before.getBoolean("canUndo"),after.getBoolean("canUndo")); assertTrue(after.isNull("renderError"))
        } } finally { NativeBridge.destroy(id) }
    }
    @Test fun exhaustedPreviewRetainsLastFrameAndDoesNotRetryEveryCall() {
        val root = File(context.filesDir,"memory-backend/${UUID.randomUUID()}").apply { mkdirs() }
        val id = create(root,JSONArray(),values(layer(1,32768,JSONObject().put("kind","solid").put("color",values(1,0,0,1)),true)))
        try { surface(id) {
            data(NativeBridge.configureMemory(id,0,true)); data(NativeBridge.previewMode(id,1,0))
            awaitFrame(id,0.0)
            val packages = data(NativeBridge.plugin(id,JSONObject().put("op","list").toString())).getJSONArray("packages")
            val pkg = (0 until packages.length()).map(packages::getJSONObject).first { p ->
                val effects = p.getJSONObject("manifest").getJSONArray("effects")
                (0 until effects.length()).any { effects.getJSONObject(it).getString("id")=="gaussian_blur" }
            }
            val manifest = pkg.getJSONObject("manifest")
            data(NativeBridge.plugin(id,JSONObject().put("op","add").put("object",1).put("plugin",manifest.getString("id"))
                .put("version",manifest.getString("version")).put("hash",pkg.getString("hash")).put("effect","gaussian_blur").toString()))
            val before = data(NativeBridge.state(id)); val frozen = before.getJSONObject("project").toString()
            awaitFrame(id,1.0)
            val memory = data(NativeBridge.previewInfo(id)).getJSONObject("memoryAdaptation")
            assertEquals("blocked",memory.getString("status")); assertTrue(memory.getBoolean("held"))
            assertTrue(memory.getBoolean("hasLastValidFrame")); assertEquals(4,memory.getInt("refusalCount"))
            assertEquals(0.0,data(NativeBridge.state(id)).getDouble("lastPresentedFrame"),0.0)
            repeat(3) { awaitFrame(id,1.0) }
            assertEquals(4,data(NativeBridge.previewInfo(id)).getJSONObject("memoryAdaptation").getInt("refusalCount"))
            val after = data(NativeBridge.state(id))
            assertEquals(frozen,after.getJSONObject("project").toString()); assertEquals(before.getLong("revision"),after.getLong("revision"))
            assertTrue(after.isNull("renderError"))
        } } finally { NativeBridge.destroy(id) }
    }
}
