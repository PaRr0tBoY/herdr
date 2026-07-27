# Bug修复
1. 框选对比度丢失 done
2. shift enter换行失败 done



# 新功能
1. 新标签页选择agent done
2. 新标签页继承agent done
   对话标题，标题格式：数字 +
   agent标题，例如：2 登录界面重构
3. 在工作目录右侧显示代码变更
4. 临时笔记本 done


git clone https://github.com/ogulcancelik/herdr
cd herdr
cargo build --release

just test        # unit tests
just check       # formatting, tests, and maintenance checks
# 

