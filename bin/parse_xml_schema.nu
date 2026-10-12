#!/usr/bin/env nu

def main [input: path] {
    let xsd_output = $input | path parse | update extension xsd | path join
    #let json_output = $input | path parse | update extension json | path join
    trang $input $xsd_output
    open $xsd_output
}
