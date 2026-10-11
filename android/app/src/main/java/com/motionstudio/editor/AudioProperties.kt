package com.motionstudio.editor

import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.*

@Composable internal fun AudioProperties(vm:EditorViewModel,modifier:Modifier) {
    val objectId=vm.selected;val audio=vm.audioClip(objectId)
    var dragging by remember{mutableStateOf(false)}
    var curve by remember(objectId){mutableStateOf(false)}
    var volume by remember(objectId){mutableFloatStateOf(audio?.optDouble("volume",1.0)?.toFloat()?:1f)}
    LaunchedEffect(objectId,audio?.optDouble("volume")){if(!dragging)volume=audio?.optDouble("volume",1.0)?.toFloat()?:1f}
    LaunchedEffect(objectId,audio?.optLong("asset")){vm.requestWaveform(objectId)}
    if(curve) {
        Column(modifier.background(Panel)) {
            TextButton(onClick={curve=false},modifier=Modifier.heightIn(min=48.dp)){Text("返回声音参数")}
            CurveEditor(vm,Modifier.fillMaxWidth().weight(1f))
        }
        return
    }
    Column(modifier.background(Panel).padding(horizontal=12.dp).verticalScroll(rememberScrollState()).testTag("audio-properties")) {
        Row(Modifier.fillMaxWidth().heightIn(min=48.dp),verticalAlignment=Alignment.CenterVertically) {
            if(vm.contentKind()=="video")Tool(Icons.Default.ArrowBack,"返回视频变换"){vm.property="position"}
            Text(if(vm.contentKind()=="video")"视频原声"else objectName(vm,objectId),Modifier.weight(1f),color=Ink,maxLines=1,overflow=TextOverflow.Ellipsis)
            Tool(Icons.Default.Close,"关闭声音面板"){vm.panelOpen=false}
        }
        if(audio==null){Text("此视频没有原声",color=Muted);return@Column}
        Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically){Text("音量",Modifier.weight(1f),color=Muted);Text("${(volume*100).roundToInt()}%",color=Ink)
            Switch(!audio.optBoolean("muted"),{vm.setAudio(objectId,muted=!it)},enabled=vm.editable(),modifier=Modifier.testTag("audio-enabled"))}
        StudioSlider(volume,{value->if(!dragging){vm.beginGesture();dragging=true};volume=value;vm.setAudio(objectId,volume=value.toDouble(),save=false)},valueRange=0f..2f,enabled=vm.editable(),
            onValueChangeFinished={if(dragging){vm.endGesture();dragging=false}},modifier=Modifier.heightIn(min=48.dp).testTag("audio-volume"))
        if(vm.state.sample?.optJSONObject("capabilities")?.optJSONObject("audio")?.optJSONObject("levels")?.optBoolean("supported")==true) {
            AudioLevelWheel(vm,"left","左声道（dB）")
            AudioLevelWheel(vm,"right","右声道（dB）")
            Row(Modifier.fillMaxWidth().heightIn(min=48.dp),verticalAlignment=Alignment.CenterVertically) {
                Tool(Icons.Default.SkipPrevious,"上一音量关键帧",vm.keys().any{it.getInt("frame")<vm.frame}){vm.jumpKey(false)}
                Box(Modifier.testTag("audio-level-key")){Tool(if(vm.currentKey()!=null)Icons.Default.Diamond else Icons.Default.Add,"音量关键帧",
                    vm.editable()&&audioLevelChannel(vm.property)!=null,vm::toggleKey)}
                Tool(Icons.Default.SkipNext,"下一音量关键帧",vm.keys().any{it.getInt("frame")>vm.frame}){vm.jumpKey(true)}
                TextButton(onClick={curve=true},enabled=vm.editable()&&audioLevelChannel(vm.property)!=null&&vm.easingSegment()!=null,
                    modifier=Modifier.testTag("audio-level-curve")){Text("缓动")}
            }
        }
        val waveform=vm.waveforms[objectId]?.optJSONArray("buckets")
        Canvas(Modifier.fillMaxWidth().height(64.dp).testTag("audio-waveform")) {
            drawLine(Muted.copy(alpha=.25f),Offset(0f,size.height/2),Offset(size.width,size.height/2),1f)
            waveform?.let{b->for(i in 0 until b.length()){
                val bucket=b.getJSONObject(i);val x=size.width*i/b.length();drawLine(if(audio.optBoolean("muted"))Muted else Accent,Offset(x,size.height*(.5f-bucket.getDouble("max").toFloat()*.45f)),Offset(x,size.height*(.5f-bucket.getDouble("min").toFloat()*.45f)),1f)
            }}
        }
        Text("长按时间轴片段移动；拖动两端裁剪。",color=Muted,fontSize=12.sp,lineHeight=18.sp)
        Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween){TextButton(onClick=vm::undo,enabled=vm.state.canUndo,modifier=Modifier.height(48.dp)){Text("撤销")};TextButton(onClick=vm::splitClip,enabled=vm.canSplitClip(),modifier=Modifier.height(48.dp)){Text("当前帧分割")};TextButton(onClick=vm::duplicate,enabled=vm.editable(),modifier=Modifier.height(48.dp)){Text("创建副本")}}
    }
    DisposableEffect(objectId){onDispose{if(dragging)vm.cancelGesture()}}
}

@Composable private fun AudioLevelWheel(vm:EditorViewModel,channel:String,label:String) {
    val range=vm.state.sample?.optJSONObject("capabilities")?.optJSONObject("audio")?.optJSONObject("levels")?.optJSONArray("range")?:return
    val minimum=range.getDouble(0);val maximum=range.getDouble(1)
    val key="audio:$channel";val value=vm.audioLevelValue(vm.selected,key)?:0.0
    var draft by remember(vm.selected,key){mutableStateOf<Double?>(null)}
    var gesture by remember(vm.selected,key){mutableStateOf(false)}
    var at by remember{mutableIntStateOf(0)}
    var input by remember{mutableStateOf(false)}
    fun set(next:Double,save:Boolean){vm.property=key;vm.setPropertyValue(vm.selected,key,at,next,save)}
    Row(Modifier.fillMaxWidth().heightIn(min=56.dp),verticalAlignment=Alignment.CenterVertically) {
        TextButton(onClick={vm.pause();vm.property=key},modifier=Modifier.width(100.dp).heightIn(min=48.dp).testTag("audio-level-select-$channel")){
            Text(label,color=if(vm.property==key)Accent else Ink,fontSize=13.sp)}
        NumericWheel(draft?:value,minimum,maximum,label,vm.editable(),Modifier.weight(1f).testTag("audio-level-wheel-$channel"),
            onValueChange={next->if(!gesture){at=floor(vm.frame).toInt();vm.beginGesture();gesture=true};draft=next;set(next,false)},
            onFinished={if(gesture)vm.endGesture{draft=null};gesture=false},
            onCancelled={if(gesture)vm.cancelGesture();gesture=false;draft=null},inertiaGroup=vm.gestureInertia)
        TextButton(onClick={input=true},enabled=vm.editable(),modifier=Modifier.widthIn(min=72.dp).heightIn(min=48.dp).testTag("audio-level-value-$channel")){
            Text(String.format(java.util.Locale.US,"%.1f",draft?:value),fontSize=12.sp)}
    }
    if(input)InputDialog(label,value.toString(),{input=false},true){raw->val next=raw.toDoubleOrNull();if(next==null||!next.isFinite()||next !in minimum..maximum)vm.showOperationError("音量范围为 $minimum 至 $maximum dB")
        else{at=floor(vm.frame).toInt();set(next,true)}}
    DisposableEffect(vm.selected,key){onDispose{if(gesture)vm.cancelGesture()}}
}
