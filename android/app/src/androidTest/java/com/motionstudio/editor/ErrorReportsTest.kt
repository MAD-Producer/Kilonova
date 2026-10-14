package com.motionstudio.editor

import android.app.Application
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.nio.ByteBuffer
import java.util.UUID

class ErrorReportsTest {
    class ThrowingReadOnly {
        fun isReadOnly():Boolean=throw IllegalStateException("JNI 诊断异常")
    }
    class BrokenDiagnostic:RuntimeException("原始异常消息") {
        override fun toString():String=throw IllegalStateException("secondary diagnostic failure")
    }
    class ThrowingBrokenDiagnostic {
        fun isReadOnly():Boolean=throw BrokenDiagnostic()
    }
    class ThrowingLongDiagnostic {
        fun isReadOnly():Boolean=throw IllegalStateException("中文😀".repeat(3000))
    }
    private fun probe(kind:Int,value:Any?):JSONObject=JSONObject(NativeBridge.diagnosticJniException(kind,value))
    private fun recovered() {
        // Same thread and the ordinary JNI bridge, not another diagnostic call.
        val next=JSONObject(NativeBridge.projectTemplate(0))
        assertTrue(next.toString(),next.getBoolean("ok"))
    }
    @Test fun javaExceptionsReturnStructuredDiagnosticsAndLeaveTheNextCallUsable() {
        val thrown=probe(1,ThrowingReadOnly())
        assertFalse(thrown.toString(),thrown.getBoolean("ok"))
        assertTrue(thrown.toString(),thrown.getString("error").contains("IllegalStateException: JNI 诊断异常"))
        recovered()
        val missing=probe(1,Any())
        assertFalse(missing.toString(),missing.getBoolean("ok"))
        assertTrue(missing.toString(),missing.getString("error").contains("NoSuchMethodError"))
        recovered()
    }
    @Test fun diagnosticRenderingExceptionsKeepTheOriginalErrorAndClearSecondaryExceptions() {
        val broken=probe(1,ThrowingBrokenDiagnostic())
        assertFalse(broken.toString(),broken.getBoolean("ok"))
        assertTrue(broken.toString(),broken.getString("error").contains("BrokenDiagnostic"))
        assertTrue(broken.toString(),broken.getString("error").contains("原始异常消息"))
        assertFalse(broken.toString(),broken.getString("error").contains("secondary diagnostic failure"))
        recovered()
        val long=probe(1,ThrowingLongDiagnostic())
        assertFalse(long.toString(),long.getBoolean("ok"))
        // Kotlin counts UTF-16 units; the Rust limit counts Unicode characters.
        assertTrue(long.toString(),long.getString("error").length<=4096)
        recovered()
    }
    @Test fun safeStringAndDirectBufferChecksPreserveTheirSuccessAndFailureChannels() {
        val invalid=probe(0,Any())
        assertFalse(invalid.toString(),invalid.getBoolean("ok"))
        assertTrue(invalid.toString(),invalid.getString("error").contains("java.lang.String"))
        recovered()
        val text=probe(0,"正常字符串😀")
        assertTrue(text.toString(),text.getBoolean("ok"))
        assertEquals("正常字符串😀",text.getJSONObject("data").getString("text"))
        val heap=probe(2,ByteBuffer.allocate(8))
        assertFalse(heap.toString(),heap.getBoolean("ok"))
        recovered()
        val direct=probe(2,ByteBuffer.allocateDirect(8))
        assertTrue(direct.toString(),direct.getBoolean("ok"))
        assertEquals(8,direct.getJSONObject("data").getInt("capacity"))
        val readOnly=probe(2,ByteBuffer.allocateDirect(8).asReadOnlyBuffer())
        assertFalse(readOnly.toString(),readOnly.getBoolean("ok"))
        assertTrue(readOnly.toString(),readOnly.getString("error").contains("read-only"))
        recovered()
    }

    @Test fun reportsRedactLocationsAndSecretsAndSurviveRestartWithinRetentionLimit() {
        val app=InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as Application
        val folder=File(app.cacheDir,"reports-test-${UUID.randomUUID()}")
        val reports=ErrorReports(app,folder)
        repeat(60){reports.operation("edit:set_scalar")}
        val file=reports.record("import failed content://secret/clip.mp4 /storage/emulated/0/private.mp4 token=private-token",IllegalStateException("file:///private/clip.mp4"))!!
        val text=file.readText();val json=JSONObject(text)
        assertFalse(text.contains("private-token"));assertFalse(text.contains("content://secret"));assertFalse(text.contains("private.mp4"));assertFalse(text.contains("file:///private"))
        assertFalse(json.getBoolean("includes_project_or_media"));assertEquals(40,json.getJSONArray("recent_operations").length());assertTrue(json.has("stack"))
        repeat(12){reports.record("error $it")}
        assertEquals(8,folder.listFiles()!!.count{it.extension=="json"})
        assertEquals(reports.latest()!!.name,ErrorReports(app,folder).export().name)
    }
    @Test fun duplicateFailuresAreCoalescedAndOnlyOperationNamesAreCaptured() {
        val app=InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as Application
        val folder=File(app.cacheDir,"reports-test-${UUID.randomUUID()}")
        val reports=ErrorReports(app,folder)
        val first=reports.record("repeat")!!;repeat(10){reports.record("repeat")}
        assertEquals(1,folder.listFiles()!!.count{it.extension=="json"});assertEquals(first,reports.latest())
        assertEquals("password=[已隐藏] api-key=[已隐藏]",redactDiagnostic("password=secret api-key:secret"))
    }
}
