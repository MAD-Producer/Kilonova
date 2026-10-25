package com.motionstudio.editor

import java.nio.ByteBuffer

/** v12 adds explicit numeric LUT encoding without changing physical slots. */
internal object RenderPlanBudget {
    const val VERSION=12
    const val DENSE_CURVE_SAMPLES=4081
    const val VECTOR_RECORD_BYTES=40
    const val HEADER_BYTES=160
    const val SCRATCH_RECORD_BYTES=32
    const val MAX_SCRATCH_PASSES=128*(16*(8+3)+1)
    data class LutLayout(val samples:Int,val pixelBytes:Int,val floating:Boolean) {
        val stride get()=samples*pixelBytes
    }
    fun lutLayout(plan:ByteBuffer):LutLayout {
        scratchBytes(plan)
        val layout=if(plan.getInt(156)==0)LutLayout(256,4,false)else LutLayout(DENSE_CURVE_SAMPLES,16,true)
        val offset=plan.getInt(44);val count=plan.getInt(48);val total=plan.getInt(28)
        check(total in HEADER_BYTES..plan.capacity()&&offset>=HEADER_BYTES&&offset%4==0&&count in 0..128*16&&offset.toLong()+count.toLong()*layout.stride<=total){"曲线数值表范围失效"}
        return layout
    }
    data class ScratchAllocation(val width:Int,val height:Int,val format:Int,
        val workingSpaces:Int,val alphaModes:Int,val usage:Int,val bytesPerPixel:Int) {
        fun bytes():Long=width.toLong()*height*bytesPerPixel
        fun sameStorage(other:ScratchAllocation)=width==other.width&&height==other.height&&format==other.format&&usage==other.usage
    }
    fun scratchAllocations(plan:ByteBuffer):List<ScratchAllocation> {
        scratchBytes(plan)
        val total=plan.getInt(28);val offset=plan.getInt(140);val count=plan.getInt(144)
        check(total in HEADER_BYTES..plan.capacity()&&offset>=HEADER_BYTES&&offset%4==0&&count in 0..MAX_SCRATCH_PASSES&&plan.getInt(148)==SCRATCH_RECORD_BYTES&&offset.toLong()+count.toLong()*SCRATCH_RECORD_BYTES<=total){"效果临时纹理表失效"}
        return List(count){i->
            val p=offset+i*SCRATCH_RECORD_BYTES
            val a=ScratchAllocation(plan.getInt(p),plan.getInt(p+4),plan.getInt(p+8),
                plan.getInt(p+12),plan.getInt(p+16),plan.getInt(p+20),plan.getInt(p+24))
            // Semantic bit sets describe separate, fully cleared value lifetimes.
            check(a.width>0&&a.height>0&&a.format in 0..4&&a.workingSpaces in 1..3&&a.alphaModes in 1..3&&a.usage==15&&a.bytesPerPixel==(when(a.format){2,3->8;4->16;else->4})&&plan.getInt(p+28)==0){"效果临时纹理描述失效"}
            check(when(plan.getInt(152)){0->a.format in 0..2;1->a.format==3;2->a.format==2;3->a.format==4;else->false}){"效果纹理格式与工作精度不一致"}
            check(a.width.toLong()*a.height<=Long.MAX_VALUE/a.bytesPerPixel){"效果临时纹理容量溢出"}
            a
        }
    }
    fun compositingOffset(plan:ByteBuffer):Int {
        scratchBytes(plan)
        val count=plan.getInt(132);val offset=plan.getInt(128);val total=plan.getInt(28)
        check(count==plan.getInt(8)&&count in 0..128&&plan.getInt(136)==20&&offset>=HEADER_BYTES&&offset.toLong()+count*20L<=total&&total<=plan.capacity()){"图层混合记录失效"}
        repeat(count){i->val p=offset+i*20;check(plan.getInt(p) in 0..14&&plan.getInt(p+4) in 0..1&&plan.getInt(p+8) in -1 until count&&plan.getInt(p+12) in 0..4&&plan.getInt(p+16) in 0..1){"图层混合值失效"}}
        return offset
    }
    fun scratchBytes(plan:ByteBuffer):Long {
        check(plan.capacity()>=HEADER_BYTES&&plan.getInt(0)==0x46584d53&&plan.getInt(4)==VERSION){"不兼容的帧预算协议"}
        val precision=plan.getInt(152);val encoding=plan.getInt(156)
        check(precision in 0..3&&encoding==(if(precision==0)0 else 1)){"帧工作精度与曲线编码协议失效"}
        val bytes=plan.getInt(76).toLong()
        check(bytes>0){"效果临时纹理预算失效"}
        return bytes
    }
}
