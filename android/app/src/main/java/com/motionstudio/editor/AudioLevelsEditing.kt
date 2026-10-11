package com.motionstudio.editor

import org.json.JSONArray
import org.json.JSONObject

internal fun audioLevelChannel(key:String):String?=key.removePrefix("audio:").takeIf{key.startsWith("audio:")&&it in listOf("left","right")}
internal fun EditorViewModel.audioLevelTrackRaw(objectId:Long,key:String):JSONObject? {
    val channel=audioLevelChannel(key)?:return null
    val layer=layer(objectId)?:return null
    if(layer.getJSONObject("content").optString("kind") !in listOf("audio","video","composition"))return null
    return layer.optJSONObject("audio_levels")?.optJSONObject(channel)
        ?:JSONObject().put("value",0.0).put("keys",JSONArray())
}
internal fun EditorViewModel.audioLevelValue(objectId:Long,key:String):Double? {
    val channel=audioLevelChannel(key)?:return null
    return state.sample?.optJSONArray("sampledLayers").objects().firstOrNull{it.getLong("id")==objectId}
        ?.optJSONObject("audio_levels")?.optDouble(channel)
        ?:audioLevelTrackRaw(objectId,key)?.optDouble("value")
}
internal fun EditorViewModel.routeAudioLevelCommand(command:JSONObject):JSONObject? {
    val channel=audioLevelChannel(command.optString("property"))?:return command
    val action=JSONObject().put("channel",channel)
    when(val op=command.getString("op")) {
        "set_scalar"->{action.put("kind","set").put("frame",command.getInt("frame")).put("value",command.getDouble("value"));if(command.has("animated"))action.put("animated",command.getBoolean("animated"))}
        "animate"->action.put("kind","animate").put("frame",command.getInt("frame")).put("enabled",command.getBoolean("enabled"))
        "delete_key"->action.put("kind",op).put("frame",command.getInt("frame"))
        "move_key","copy_key"->action.put("kind",op).put("from",command.getInt("from")).put("to",command.getInt("to"))
        "ease","curve"->action.put("kind","curve").put("frame",command.getInt("frame")).put("easing",command.optJSONObject("easing")?:JSONObject().put("ease",command.getString("ease")))
        else->{showOperationError("此音量轨道操作暂不可用");return null}
    }
    return JSONObject().put("op","audio_levels").put("object",command.getLong("object")).put("action",action)
}
