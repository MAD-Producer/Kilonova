package com.motionstudio.editor

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import java.nio.ByteBuffer
import java.nio.ByteOrder

class RenderPlanBudgetTest {
    private fun plan(bytes:Int,version:Int=RenderPlanBudget.VERSION)=ByteBuffer.allocate(RenderPlanBudget.HEADER_BYTES).order(ByteOrder.nativeOrder()).apply {
        putInt(0,0x46584d53);putInt(4,version);putInt(76,bytes)
    }
    @Test fun readsAllNativeTiersWithoutAnIndependentScratchConstant() {
        for(mib in listOf(64,96,192,384))assertEquals(mib.toLong()*1024*1024,RenderPlanBudget.scratchBytes(plan(mib*1024*1024)))
    }
    @Test fun rejectsMissingBudgetOldProtocolAndShortHeader() {
        assertThrows(IllegalStateException::class.java){RenderPlanBudget.scratchBytes(plan(0))}
        assertThrows(IllegalStateException::class.java){RenderPlanBudget.scratchBytes(plan(64*1024*1024,5))}
        assertThrows(IllegalStateException::class.java){RenderPlanBudget.scratchBytes(plan(64*1024*1024,9))}
        assertThrows(IllegalStateException::class.java){RenderPlanBudget.scratchBytes(ByteBuffer.allocate(76))}
    }
    private fun physical(vararg entries:IntArray):ByteBuffer = ByteBuffer.allocate(
        RenderPlanBudget.HEADER_BYTES+entries.size*RenderPlanBudget.SCRATCH_RECORD_BYTES
    ).order(ByteOrder.nativeOrder()).apply {
        putInt(0,0x46584d53);putInt(4,RenderPlanBudget.VERSION);putInt(28,capacity());putInt(76,64*1024*1024)
        putInt(140,RenderPlanBudget.HEADER_BYTES);putInt(144,entries.size);putInt(148,32)
        entries.forEachIndexed { i,entry -> entry.forEachIndexed { j,v -> putInt(RenderPlanBudget.HEADER_BYTES+i*32+j*4,v) } }
    }
    private val rgba8 get()=intArrayOf(16,32,0,1,1,15,4,0)
    @Test fun readsPhysicalExtentsFormatsAndSemanticClassesWithoutSlotAssumptions() {
        val allocations=RenderPlanBudget.scratchAllocations(physical(
            rgba8,intArrayOf(32,16,1,2,2,15,4,0),intArrayOf(8,8,2,1,1,15,8,0)))
        assertEquals(listOf(2048L,2048L,512L),allocations.map{it.bytes()})
        assertEquals(listOf(0,1,2),allocations.map{it.format})
        assertEquals(listOf(1,2,1),allocations.map{it.alphaModes})
        val clearedLifetimes=RenderPlanBudget.scratchAllocations(physical(rgba8.apply{this[3]=3;this[4]=3})).single()
        assertEquals(3,clearedLifetimes.workingSpaces)
        assertEquals(3,clearedLifetimes.alphaModes)
        assertEquals(true,allocations[0].sameStorage(clearedLifetimes))
        assertEquals(emptyList<RenderPlanBudget.ScratchAllocation>(),RenderPlanBudget.scratchAllocations(physical()))
    }
    @Test fun rejectsBadPhysicalTableRangesStrideAndUnboundedCounts() {
        for((field,value) in listOf(28 to 159,28 to 4096,140 to 0,140 to 161,
            140 to Int.MAX_VALUE,144 to -1,144 to RenderPlanBudget.MAX_SCRATCH_PASSES+1,148 to 16)) {
            val buffer=physical(rgba8).apply{putInt(field,value)}
            assertThrows(IllegalStateException::class.java){RenderPlanBudget.scratchAllocations(buffer)}
        }
    }
    @Test fun rejectsIncompatibleSemanticsFormatByteSizesAndCapacityOverflow() {
        for((field,value) in listOf(0 to 0,1 to -1,2 to 3,3 to 0,4 to 4,5 to 0,6 to 8,7 to 1)) {
            val descriptor=rgba8.apply{this[field]=value}
            assertThrows(IllegalStateException::class.java){RenderPlanBudget.scratchAllocations(physical(descriptor))}
        }
        assertThrows(IllegalStateException::class.java){RenderPlanBudget.scratchAllocations(
            physical(intArrayOf(Int.MAX_VALUE,Int.MAX_VALUE,2,1,1,15,8,0)))}
    }
    @Test fun readsFourPrecisionsAndRejectsImplicitPrecisionFallback() {
        for((precision,format,pixelBytes) in listOf(Triple(0,0,4),Triple(1,3,8),Triple(2,2,8),Triple(3,4,16))) {
            val descriptor=intArrayOf(17,3,format,1,1,15,pixelBytes,0)
            val buffer=physical(descriptor).apply{putInt(152,precision)}
            assertEquals(51L*pixelBytes,RenderPlanBudget.scratchAllocations(buffer).single().bytes())
            if(precision!=0) {
                val downgraded=physical(rgba8).apply{putInt(152,precision)}
                assertThrows(IllegalStateException::class.java){RenderPlanBudget.scratchAllocations(downgraded)}
            }
        }
        for(code in listOf(-1,4,Int.MAX_VALUE)) {
            assertThrows(IllegalStateException::class.java){RenderPlanBudget.scratchBytes(plan(64*1024*1024).apply{putInt(152,code)})}
        }
        assertThrows(IllegalStateException::class.java){RenderPlanBudget.scratchBytes(plan(64*1024*1024,10))}
    }

}
